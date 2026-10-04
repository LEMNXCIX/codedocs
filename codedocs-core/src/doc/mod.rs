//! The document intermediate representation: Markdown events → [`DocBlock`].
//!
//! # Why this module exists
//!
//! [`crate::markdown::render_markdown`] returns **HTML**, because in the Tauri
//! app a webview painted it. GPUI has no webview, so the HTML is useless there.
//!
//! Rewriting the parser into the GPUI crate would have been the obvious move and
//! the wrong one, for a reason that is *historical*: `codedocs-core` was created
//! because there used to be **two** `render_markdown` implementations that could
//! drift apart. One renderer was the point; two were the bug. A second parser in
//! the GPUI crate is two parsers.
//!
//! So this module is the fix for both problems at once. It is a **contract
//! between parsing and painting**: the core emits a structure, and every frontend
//! translates it to whatever it draws. HTML for Tauri, GPUI elements for the new
//! app. The renderer in `codedocs-gpui` is a `match` over `DocBlock` and nothing
//! else, so all the hard decisions — the URL allowlist, unique slugs — stay in the
//! core where they are already tested.
//!
//! # The threat model is unchanged, and that is the point
//!
//! The preview renders *untrusted files the user opened*. Markdown permits raw
//! HTML and `pulldown-cmark` passes it through verbatim. The HTML renderer
//! handles that with an allowlist by construction (see the module docs of
//! [`crate::markdown`]); this module replicates the same construction for the IR:
//!
//! 1. Every `Event::Html` / `Event::InlineHtml` **from the document** is dropped.
//!    No field of any type here holds an HTML string, so there is nothing for a
//!    renderer to interpolate.
//! 2. Link and image destinations go through [`is_safe_url`], and the result is
//!    stored in [`SpanMarks::Link`]. **A rejected URL becomes `None`, not the
//!    string.** That is the whole security argument, and it is structural rather
//!    than procedural: a GPUI renderer that only knows how to draw
//!    `Link { url: Some(u) }` and `Link { url: None }` has no code path that can
//!    emit `javascript:`, because the field is an `Option<String>` that is already
//!    `None`. Not "the renderer must remember to check". There is nothing to
//!    remember.
//! 3. [`DocBlock::Opaque`] keeps the *text* of a dropped HTML block so the reader
//!    does not silently lose content.
//!
//! # `Box` in a recursive enum
//!
//! `DocBlock::List` and `DocBlock::Quote` hold `Vec<DocBlock>`, so the type is
//! recursive: it contains itself.
//!
//! Rust requires every type to have a **known, finite size**, and a struct cannot
//! contain itself, because then its size would never resolve. `Vec<T>` is the
//! exception: its size is three words — pointer, length, capacity — no matter what
//! `T` is, because the elements live on the **heap** and the `Vec` holds only a
//! handle to them.
//!
//! So `Vec<DocBlock>` inside `DocBlock` is legal, and that is the trick that makes
//! recursion work at all. What is **not** legal is a bare `DocBlock` field:
//!
//! ```compile_fail
//! enum Tree { Leaf, Node(Tree, Tree) }   // E0072: recursive type has infinite size
//! enum Tree { Leaf, Node(Box<Tree>, Box<Tree>) }   // ok
//! ```
//!
//! The `E0072` is the concept central to this milestone, and the guide chapter
//! carries the real compiler output.
//!
//! And this is worth being precise about, because it is easy to over-generalise:
//! with `Vec<DocBlock> items`, the `Box` is **already inside the `Vec`**'s heap
//! allocation, so this particular type would compile *without* any `Box` at all.
//! `Box` is what you need for the **direct** self-reference, and the two designs
//! are genuinely different:
//!
//! | shape | `Box` needed? | what it buys |
//! |---|---|---|
//! | `List { items: Vec<Vec<DocBlock>> }` | no | a list item holds *several* blocks |
//! | `List { item: Box<DocBlock> }` | yes | one block per item, and the `Box` keeps the size finite |
//!
//! This IR uses the first shape, so [`DocBlock`] as written has no `Box`. See
//! [`ListItemsExt::en_list`] for the second shape, which does, and for the test
//! that proves the `E0072` is real rather than folklore.

use std::collections::HashMap;

use pulldown_cmark::{Event, Parser, Tag, TagEnd};

use crate::markdown::{fenced_lang, is_safe_url, parser_options, unique_slug};

/// One block-level element of a parsed document.
///
/// Every variant is **data, not behaviour**. There is no `match` over strings, no
/// HTML anywhere, and no URL that has not been through the allowlist. A frontend
/// that cannot draw one of these variants has to say so in its own `match`, and
/// the compiler will point at the missing arm.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocBlock {
    /// A heading. `id` is the anchor the preview emits and the outline links to.
    Heading {
        /// 1 through 6, as reported by the parser.
        level: u8,
        /// Unique within the document. See [`crate::markdown::slugify`].
        ///
        /// The uniqueness is not decoration: two headings with the same text must
        /// get different ids, or an outline with two "Notes" entries scrolls both
        /// to the first one.
        id: String,
        /// The inline content, already flattened to one plain span.
        ///
        /// Flattened and not `Vec<Span>`: an outline label is navigation, and a
        /// label with a link inside it has two click targets. This is the same
        /// trade `render_markdown` makes, and for the same reason.
        spans: Vec<Span>,
    },
    /// A run of inline content.
    Paragraph { spans: Vec<Span> },
    /// A fenced or indented code block.
    CodeBlock {
        /// The language from the fence info string, lowercased.
        ///
        /// `None` for an indented block or a fence with no info string. A renderer
        /// is free to ignore it; it is here because the app wants to highlight,
        /// not because the IR needs it.
        lang: Option<String>,
        /// The code, verbatim. Never math-split and never escaped: it is not
        /// markup.
        code: String,
    },
    /// A bullet or numbered list.
    List {
        /// `true` for `1.`, `false` for `-`.
        ordered: bool,
        /// One entry per item. Each item is a sequence of blocks, because an item
        /// can hold a paragraph **and** a nested list.
        items: Vec<Vec<DocBlock>>,
    },
    /// A GFM table.
    Table {
        /// One `Vec<Span>` per header cell.
        header: Vec<Vec<Span>>,
        /// `rows[r][c]` is the cell at row `r`, column `c`.
        rows: Vec<Vec<Vec<Span>>>,
    },
    /// A blockquote. `> text` produces one of these with a paragraph inside.
    Quote { blocks: Vec<DocBlock> },
    /// A thematic break: `---`, `***`, `___`.
    Rule,
    /// A block that was **not** rendered, kept only for its readable text.
    ///
    /// It is what an `HtmlBlock` becomes, and it is the difference between "this
    /// document had raw HTML and we dropped it" and "this document had raw HTML
    /// and it silently vanished". The text is preserved; the markup is not.
    ///
    /// This variant only accumulates `Event::Text`, which for an HTML block is
    /// the text *between* the tags — not a strip of the markup itself. A pure
    /// `<script>` block yields an empty `Opaque`, which is correct: there is no
    /// readable text in it.
    Opaque { text: String },
}

/// A run of text with uniform formatting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    /// The visible text.
    pub text: String,
    /// What formatting it carries.
    pub marks: SpanMarks,
}

impl Span {
    /// A span with no formatting.
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            marks: SpanMarks::None,
        }
    }

    /// A span with one kind of formatting.
    pub fn marked(text: impl Into<String>, marks: SpanMarks) -> Self {
        Self {
            text: text.into(),
            marks,
        }
    }

    /// The visible text, borrowed.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The formatting.
    pub fn marks(&self) -> &SpanMarks {
        &self.marks
    }

    /// A link span whose URL passed the allowlist.
    pub fn link(text: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            marks: SpanMarks::Link {
                url: Some(url.into()),
            },
        }
    }

    /// A link span whose URL **did not** pass the allowlist.
    ///
    /// Existe como constructor para que el significado de [`SpanMarks::Link`]
    /// con `None` quede escrito en el código de los tests y no solo en la
    /// documentación. Un `Some(...)` explícito sería el camino fácil, y sería
    /// también el que documenta mal el modelo de seguridad.
    pub fn link_rechazada(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            marks: SpanMarks::Link { url: None },
        }
    }
}

/// The formatting on a [`Span`].
///
/// This is an enum and not a bitfield (`bold | italic | code`) on purpose: **a
/// bitfield can represent combinations and this cannot**, and the question is
/// what happens to `**bold _and_ italic**`.
///
/// The IR chose to resolve it: the marks are a stack while the span is open, and
/// **the first one opened wins**. That is a property of the *outer* markup, so
/// `**a _b_ c**` gives bold and `*a **b** c*` gives italic. La regla se lee como
/// "el énfasis exterior es el primero que siente quien lee", que es como se ven
/// los dos en cualquier renderer de Markdown.
///
/// La regla quedó escrita **después** de medir las dos alternativas. La primera
/// versión del doc decía "gana negrita" y su test afirmaba que `*a **b** c*` daba
/// negrita; el código, en cambio, aplicaba "la primera que se abrió". El test
/// `tests/doc_ir.rs::la_primera_marca_abierta_gana` es el que detectó que las dos
/// frases no dicen lo mismo.
///
/// The lossless alternative is `Vec<SpanMark>` with one item per mark. That is
/// what a general-purpose renderer wants, and it is not what this wants: every
/// consumer is CodeDocs, and CodeDocs draws one emphasis per run. The trade is
/// legitimate **because both sides of it live in this repository** — the IR and
/// the renderers — so they can be changed together. An IR shared with someone
/// else's renderer would need the `Vec`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum SpanMarks {
    #[default]
    None,
    Bold,
    Italic,
    Code,
    Strike,
    /// A link.
    ///
    /// The destination travels **separately from the text**, and it is an
    /// `Option`, and that is the entire security model of this IR. See the module
    /// docs.
    ///
    /// `None` does not mean "no link". It means "the document asked for a link
    /// whose URL did not pass [`is_safe_url`]". The text is still there, because
    /// dropping the reader's content would be a worse failure than losing the
    /// navigation.
    Link {
        /// The URL, only if it passed the allowlist.
        url: Option<String>,
    },
}

impl SpanMarks {
    /// `true` for [`SpanMarks::None`].
    pub fn is_none(&self) -> bool {
        matches!(self, SpanMarks::None)
    }

    /// The URL if this is a link with an allowed destination.
    ///
    /// A `&str` instead of the `Option<&String>` that a field access gives, so a
    /// renderer writes `if let Some(url) = marks.url()` and not two levels of
    /// unwrapping.
    pub fn url(&self) -> Option<&str> {
        match self {
            SpanMarks::Link { url: Some(url) } => Some(url.as_str()),
            _ => None,
        }
    }

    /// `true` for [`SpanMarks::Link`], whether or not the URL survived.
    ///
    /// Es una distinción que el renderer necesita y por eso existe. En HTML un
    /// enlace sin URL es la misma etiqueta sin `href`; GPUI no tiene `href`, así
    /// que la diferencia hay que decidirla explícitamente.
    pub fn is_link(&self) -> bool {
        matches!(self, SpanMarks::Link { .. })
    }
}

/// Build a list from items that each hold several blocks.
///
/// # El `Box` que sí hace falta
///
/// [`DocBlock::List`] usa `Vec<Vec<DocBlock>>`, y ahí el `Box` ya está dentro de
/// la asignación del `Vec`, así que no hace falta ningún `Box` explícito. Esta
/// función es el otro diseño: **un bloque por ítem**, con `Box<DocBlock>` como
/// campo directo.
///
/// Y la diferencia se mide. Con `Box` el tipo compila; sin `Box`,
///
/// ```compile_fail
/// enum Tree { Leaf, Node(Box<Tree>, Box<Tree>) }  // esto sí
/// enum Tree2 { Leaf, Node(Tree2, Tree2) }         // esto da E0072
/// ```
///
/// da `error[E0072]: recursive type 'Tree2' has infinite size`, que es
/// exactamente el error que el capítulo 6 de la guía reproduce completo.
///
/// Se llama [`en_list`] y no `list` porque `List` ya es una variante del enum, y
/// un `DocBlock::list(..)`leyendo como una variante de función sería peor que un
/// nombre honesto.
pub trait ListItemsExt {
    /// Wrap `self` as the single item of a list.
    fn en_lista(self, ordered: bool) -> DocBlock;
}

impl ListItemsExt for Vec<DocBlock> {
    fn en_lista(self, ordered: bool) -> DocBlock {
        DocBlock::List {
            ordered,
            items: vec![self],
        }
    }
}

/// Parse a document into its blocks.
///
/// The counterpart of [`crate::markdown::render_markdown`] for a consumer that
/// cannot paint HTML. Same parser options, same threat model, same slug
/// allocator.
///
/// Total: it never fails and never panics, because `pulldown-cmark`'s event
/// stream is already well-formed and every unclosed construct is closed by
/// [`Builder::flush`]. That is the honest signature — there is no failure mode to
/// report, so inventing a `Result` would be noise.
pub fn parse_document(content: &str) -> Vec<DocBlock> {
    let parser = Parser::new_ext(content, parser_options());
    Builder::new().blocks(parser.into_iter())
}

/// The parsing state machine.
///
/// # Por qué un `struct` y no una función con seis parámetros
///
/// La recursión necesita una **pila**, y una pila necesita dónde vivir. Una
/// función que devuelve `Vec<DocBlock>` no puede tener una: sus locales mueren
/// cuando vuelve, y la llamada anidada necesita la pila de la llamada anterior
/// todavía intacta. Por eso los frames son un campo.
///
/// Y hay un detalle de costo que importa: el estado del parser es un frame por
/// bloque **abierto**, o sea O(profundidad) —una cita dentro de una lista dentro de
/// una cita— y no O(número de bloques). Un documento de 5 000 líneas con diez
/// listas anidadas tiene diez frames.
struct Builder {
    /// The open containers, outermost first.
    frames: Vec<Frame>,
    /// The blocks finished at top level.
    out: Vec<DocBlock>,
    /// The open inline marks, innermost last.
    ///
    /// Separate from `frames` because the two answer different questions: `frames`
    /// is "which container am I inside", `marks` is "what formatting is active".
    /// Mixing them would mean pushing a `Strong` onto the container stack, where
    /// `close()` would try to pop it as a block.
    marks: Vec<MarkFrame>,
    /// The language of each open code block, parallel to the code frames.
    ///
    /// Parallel and not a field of `Frame::Code` for one reason: a `Code` frame can
    /// be closed by `flush` when the stream ends, and a `Code` frame reached that
    /// way was opened by a `Start(CodeBlock)` — always. So a field would work too.
    /// It is a `Vec` because it keeps the pop and the take symmetric.
    langs: Vec<Option<String>>,
    /// Slug allocator, **the same one `extract_headings` uses**.
    ///
    /// Not a copy of the rule: the same type, the same call to `unique_slug`, in
    /// the same document order. That is what makes the outline and the preview use
    /// the same anchor, and it is why the two are worth writing in the same module
    /// instead of one importing the other's output.
    used_slugs: HashMap<String, usize>,
}

/// One open container on the stack.
enum Frame {
    /// A list.
    List {
        ordered: bool,
        items: Vec<Vec<DocBlock>>,
        /// The item currently being filled.
        current: Vec<DocBlock>,
    },
    /// A list item. Its blocks move into the list below on close.
    Item { blocks: Vec<DocBlock> },
    /// A blockquote.
    Quote { blocks: Vec<DocBlock> },
    /// A table being accumulated.
    Table(TableFrame),
    /// A paragraph. Its inline content becomes a `Paragraph` on close.
    Para { spans: Vec<Span> },
    /// A heading being accumulated.
    ///
    /// Headings are buffered because the `id` derives from the text, and the text
    /// is not known until the heading closes. Same reason, and same trade, as
    /// `render_markdown`: inline markup collapses to plain text.
    Heading { level: u8, text: String },
    /// A code block. Text accumulates raw.
    Code { code: String },
    /// A footnote definition, whose blocks are kept but not modelled.
    Footnote { blocks: Vec<DocBlock> },
    /// A link. Its inner spans become one `Link` span on close.
    Link {
        /// `None` when `is_safe_url` rejected the destination.
        url: Option<String>,
        spans: Vec<Span>,
    },
    /// An image. Its inner spans are its alt text.
    Image {
        /// `None` when `is_safe_url` rejected the destination.
        url: Option<String>,
        spans: Vec<Span>,
    },
    /// An HTML block. Only its text is kept.
    Opaque { text: String },
}

/// The state of an open table.
struct TableFrame {
    /// Cells of the header row.
    header: Vec<Vec<Span>>,
    /// Rows after the header.
    rows: Vec<Vec<Vec<Span>>>,
    /// The row being filled.
    current: Vec<Vec<Span>>,
    /// The cell being filled inside `current`.
    cell: Vec<Span>,
    /// `true` once `TableHead` was seen.
    ///
    /// Sin este flag el `End(TableRow)` de la fila de cabecera se contaría como una
    /// fila de datos, y la tabla tendría una fila de encabezado repetida arriba.
    seen_header: bool,
}

impl TableFrame {
    /// The finished table.
    ///
    /// Una tabla sin encabezado se **descarta** en vez de quedar con `header`
    /// vacío. Es una decisión, y es la de `render_markdown`: sin la fila de
    /// separadores `|---|---|` no hay tabla en CommonMark ni en GFM, así que
    /// `pulldown-cmark` no emite `Table` y este camino solo se alcanza con una
    /// secuencia mal formada. Un `Option` con el fallback de "primer fila como
    /// encabezado" sería más tolerante y más difícil de verificar.
    fn into_block(self) -> Option<DocBlock> {
        if !self.seen_header {
            return None;
        }
        Some(DocBlock::Table {
            header: self.header,
            rows: self.rows,
        })
    }
}

/// An open inline formatting mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MarkFrame {
    Strong,
    Emphasis,
    Strike,
}

impl MarkFrame {
    /// La marca que esta etiqueta del stack produce.
    ///
    /// El método existe para que [`Builder::marks_actuales`] sea una expresión
    /// y no un bucle. Con el `match` adentro, el "la primera gana" queda legible
    /// de un vistazo: `first()` elige el marco, este dice qué pinta.
    fn marca(self) -> SpanMarks {
        match self {
            MarkFrame::Strong => SpanMarks::Bold,
            MarkFrame::Emphasis => SpanMarks::Italic,
            MarkFrame::Strike => SpanMarks::Strike,
        }
    }
}

impl Builder {
    fn new() -> Self {
        Self {
            frames: Vec::new(),
            out: Vec::new(),
            marks: Vec::new(),
            langs: Vec::new(),
            used_slugs: HashMap::new(),
        }
    }

    /// Consume the flat event stream into a tree.
    ///
    /// `for` y no `while`, a propósito: `pulldown-cmark` garantiza que cada
    /// `Start` tiene su `End`, así que no hay un bucle "hasta que se acabe". Si un
    /// `End` cierra algo que no está en la pila —entrada mal formada—, `close` no
    /// hace nada y el evento se ignora: un documento hostil no puede hacer que el
    /// parser entre en pánico.
    fn blocks<'a>(mut self, events: impl Iterator<Item = Event<'a>>) -> Vec<DocBlock> {
        for event in events {
            self.event(event);
        }
        self.close_all();
        self.out
    }

    /// Close every frame still open when the stream ended.
    ///
    /// Unclosed fences are the interesting case: ```` ```rust ```` sin cerrar sigue
    /// siendo un bloque de código para quien lo lee, y perderlo sería perder
    /// contenido.
    fn close_all(&mut self) {
        while !self.frames.is_empty() {
            self.close();
        }
    }

    /// Close the innermost frame and hand its blocks to whoever opened it.
    fn close(&mut self) {
        match self.frames.pop() {
            Some(Frame::List {
                ordered,
                mut items,
                current,
            }) => {
                // Un item that never saw an `Item` close — o que estaba vacío — se
                // agrega igual. Un `List` con `items` vacío no es representable
                // como lista y se perdería en silencio; con esta línea no.
                if !current.is_empty() {
                    items.push(current);
                }
                self.push(DocBlock::List { ordered, items });
            }
            Some(Frame::Item { blocks }) => self.push_all(blocks),
            Some(Frame::Quote { blocks }) => {
                self.push(DocBlock::Quote { blocks });
            }
            Some(Frame::Para { spans }) => {
                if !spans.is_empty() {
                    self.push(DocBlock::Paragraph { spans });
                }
            }
            Some(Frame::Heading { level, text }) => {
                let text = text.trim().to_string();
                if !text.is_empty() {
                    let id = unique_slug(&text, &mut self.used_slugs);
                    self.push(DocBlock::Heading {
                        level,
                        id,
                        spans: vec![Span::plain(text)],
                    });
                }
            }
            Some(Frame::Code { code }) => {
                let lang = self.langs.pop().flatten();
                self.push(DocBlock::CodeBlock { lang, code });
            }
            Some(Frame::Footnote { blocks }) => self.push_all(blocks),
            Some(Frame::Table(tabla)) => {
                // `into_block` devuelve `Option`: una tabla sin fila de
                // encabezados se descarta. Un `unwrap_or` acá metería una
                // `Table` con `header` vacío en el árbol, que es un estado que
                // ningún renderer sabe pintar.
                if let Some(bloque) = tabla.into_block() {
                    self.push(bloque);
                }
            }
            Some(Frame::Link { url, spans }) => {
                let text = concat(spans);
                if !text.is_empty() {
                    self.push_span(Span::marked(text, SpanMarks::Link { url }));
                }
            }
            Some(Frame::Image { url, spans }) => {
                let text = concat(spans);
                if !text.is_empty() {
                    self.push_span(Span::marked(text, SpanMarks::Link { url }));
                }
            }
            Some(Frame::Opaque { text }) => {
                // El `trim` es necesario y no es cosmético. Medido con
                // `examples/probe.rs`: el bloque HTML que emite `pulldown-cmark`
                // incluye el salto de línea final (`Html("<div>x</div>\n")`), así
                // que sin esto el `Opaque` de un bloque de una línea termina en
                // `\n` y el preview muestra un renglón vacío de más.
                //
                // Se `trim`ean los dos extremos y no solo el final porque el
                // `Opaque` es un **bloque**: el whitespace alrededor es la
                // estructura de líneas del documento, no contenido. En cambio un
                // `Paragraph` no se `trim`ea, porque ahí el salto de línea **sí**
                // separa palabras.
                let text = text.trim().to_string();
                self.push(DocBlock::Opaque { text });
            }
            None => {}
        }
    }

    /// Append a finished block to whatever is currently open.
    ///
    /// Un `&mut self` y no un `Option<&mut Vec<_>>` a propósito: que la pila esté
    /// vacía es el caso **normal** —un bloque de primer nivel—, y manejarlo acá
    /// evita que cada call site tenga un `if let`.
    ///
    /// Y el `match` tiene un brazo para los frames que no aceptan bloques. Eso no
    /// es defensivo: un párrafo sin cerrar seguido de un bloque nuevo es entrada
    /// mal formada, y la decisión de qué hacer —convertir el bloque en hermano— es
    /// la misma que toma `render_markdown` al delegar en `push_html`.
    fn push(&mut self, block: DocBlock) {
        let destino = match self.frames.last_mut() {
            Some(Frame::List { current, .. }) => current,
            Some(Frame::Item { blocks }) => blocks,
            Some(Frame::Quote { blocks }) => blocks,
            Some(Frame::Footnote { blocks }) => blocks,
            Some(
                Frame::Para { .. }
                | Frame::Heading { .. }
                | Frame::Code { .. }
                | Frame::Table(_)
                | Frame::Link { .. }
                | Frame::Image { .. }
                | Frame::Opaque { .. },
            )
            | None => &mut self.out,
        };
        destino.push(block);
    }

    /// Append several finished blocks to whatever is currently open.
    fn push_all(&mut self, blocks: Vec<DocBlock>) {
        for block in blocks {
            self.push(block);
        }
    }

    /// Append an inline span to the innermost container that takes one.
    ///
    /// Un bloque que solo acepta `Vec<Span>` —celda de tabla, párrafo, encabezado,
    /// enlace, imagen, opaco— recibe el span. Un contenedor de bloque no, y por eso
    /// antes se llama a [`Builder::asegurar_parrafo`].
    fn push_span(&mut self, span: Span) {
        self.asegurar_parrafo();
        match self.frames.last_mut() {
            Some(Frame::Para { spans }) => spans.push(span),
            Some(Frame::Heading { text, .. }) => {
                // La misma regla de unión que `render_markdown`: un espacio, y
                // solo cuando ninguno de los dos lados ya aporta whitespace, para
                // que `**Hola** mundo` no gane dos espacios.
                if !text.is_empty()
                    && !text.ends_with(char::is_whitespace)
                    && !span.text.starts_with(char::is_whitespace)
                {
                    text.push(' ');
                }
                text.push_str(&span.text);
            }
            Some(Frame::Code { code }) => code.push_str(&span.text),
            Some(Frame::Table(table)) => table.cell.push(span),
            Some(Frame::Link { spans, .. }) | Some(Frame::Image { spans, .. }) => spans.push(span),
            Some(Frame::Opaque { text }) => text.push_str(&span.text),
            Some(Frame::List { .. })
            | Some(Frame::Item { .. })
            | Some(Frame::Quote { .. })
            | Some(Frame::Footnote { .. })
            | None => {}
        }
    }

    /// Abre un párrafo implícito si el contenedor de adentro no acepta texto inline.
    ///
    /// # El bug más caro del módulo, y el que ningún golden test habría visto
    ///
    /// El caso no es raro: es **toda lista "tensa"**, que es la forma más común de
    /// escribir una lista. Medido con `examples/probe.rs`:
    ///
    /// ```text
    /// "- uno\n- dos\n"    ->  Start(List) Start(Item) Text("uno") End(Item) …
    /// "- uno\n\n- dos\n"  ->  Start(List) Start(Item) Start(Paragraph) Text("uno") …
    /// ```
    ///
    /// La primera forma **no** tiene `Start(Paragraph)`. Sin el párrafo implícito,
    /// el IR **tiraba el texto del ítem** y una lista de tres ítems salía con tres
    /// ítems vacíos.
    ///
    /// Y el detalle que importa sobre **cómo se encontró**: el golden test de la
    /// lista con viñetas **no lo detectó**. `INSTA_UPDATE=always` regeneró el `.snap`
    /// con el árbol vacío y el test quedó verde, porque un golden test registra lo
    /// que el código hace, no lo que debería hacer. El test que compara contra
    /// `render_markdown` sí lo detectó, porque el HTML siempre tuvo el texto.
    ///
    /// Esa es la razón de existir del bloque 3 de `tests/doc_ir.rs`, y es la
    /// respuesta honesta a por qué un golden test no reemplaza un test de
    /// equivalencia.
    ///
    /// Solo `Item`: `Quote` **siempre** viene con `Start(Paragraph)` adentro —medido:
    /// `> citado` produce `Start(BlockQuote) Start(Paragraph) Text("citado")`—, y
    /// agregar brazos para casos que el parser no produce llena el código de
    /// comentarios que nadie verifica.
    fn asegurar_parrafo(&mut self) {
        let vacio = matches!(self.frames.last(), Some(Frame::Item { blocks }) if blocks.is_empty());
        if vacio {
            self.frames.push(Frame::Para { spans: Vec::new() });
        }
    }

    /// The formatting that applies to a span opened right now.
    ///
    /// Es el *stack* de marcas, no el frame del contenedor: `marks` es
    /// independiente porque una `Strong` puede empezar dentro de un párrafo, de una
    /// celda o de un enlace, y en los tres casos el texto tiene que salir con la
    /// marca puesta.
    fn marks_actuales(&self) -> SpanMarks {
        // `first()` y no un `for` con `return`: clippy dice
        // `clippy::never_loop`, y tiene razón — el `for` solo recorría el primer
        // elemento del iterador, así que escribirlo como bucle era mentir sobre
        // lo que hace. La versión con `for` fue la primera.
        self.marks
            .first()
            .map(|mark| mark.marca())
            .unwrap_or(SpanMarks::None)
    }

    /// The whole switch.
    ///
    /// # Los brazos de `Html` son el modelo de seguridad
    ///
    /// `Event::Html` y `Event::InlineHtml` que vienen del documento se
    /// **descartan**. No hay ninguna variante de [`DocBlock`] que pueda contener un
    /// string de HTML, así que un renderer no puede interpolarlo aunque quiera.
    /// Eso es "allowlist by construcción" aplicado al IR: no "sanitizamos lo que
    /// emitimos" sino "no hay campo donde emitirlo".
    fn event<'a>(&mut self, event: Event<'a>) {
        match event {
            Event::Start(Tag::Paragraph) => self.frames.push(Frame::Para { spans: Vec::new() }),
            Event::End(TagEnd::Paragraph) => self.close(),

            Event::Start(Tag::Heading { level, .. }) => self.frames.push(Frame::Heading {
                level: level as u8,
                text: String::new(),
            }),
            Event::End(TagEnd::Heading(_)) => self.close(),

            Event::Start(Tag::BlockQuote(_)) => {
                self.frames.push(Frame::Quote { blocks: Vec::new() })
            }
            Event::End(TagEnd::BlockQuote(_)) => self.close(),

            Event::Start(Tag::CodeBlock(kind)) => {
                // El idioma va con el frame, no en un `Vec` paralelo: cada
                // `Start(CodeBlock)` empuja exactamente uno y cada
                // `End(CodeBlock)` saca exactamente uno. Un `Vec` paralelo
                // admitiria que uno se desincronizara del otro; un campo, no.
                self.frames.push(Frame::Code {
                    code: String::new(),
                });
                self.langs.push(fenced_lang(&kind));
            }
            Event::End(TagEnd::CodeBlock) => self.close(),

            Event::Start(Tag::List(inicio)) => self.frames.push(Frame::List {
                // `Tag::List(Option<u64>)`: `Some(n)` es ordenada y `n` es el número
                // del primer ítem. `None` es una lista con viñetas.
                ordered: inicio.is_some(),
                items: Vec::new(),
                current: Vec::new(),
            }),
            Event::End(TagEnd::List(_)) => self.close(),

            Event::Start(Tag::Item) => self.frames.push(Frame::Item { blocks: Vec::new() }),
            Event::End(TagEnd::Item) => self.close(),

            Event::Start(Tag::FootnoteDefinition(_)) => {
                self.frames.push(Frame::Footnote { blocks: Vec::new() })
            }
            Event::End(TagEnd::FootnoteDefinition) => self.close(),

            // ---- Tables ----
            //
            // `Table` abre el frame y `TableHead` es lo que distingue la fila de
            // encabezado. Separar por `TableHead` en vez de contar filas es lo que
            // hace que `header` y `rows` salgan bien sin un índice que contar a mano.
            Event::Start(Tag::Table(_)) => self.frames.push(Frame::Table(TableFrame {
                header: Vec::new(),
                rows: Vec::new(),
                current: Vec::new(),
                cell: Vec::new(),
                seen_header: false,
            })),
            Event::End(TagEnd::Table) => self.close(),
            Event::Start(Tag::TableHead) => self.set_table(|t| t.seen_header = true),
            Event::End(TagEnd::TableHead) => self.set_table(|t| {
                t.header = std::mem::take(&mut t.current);
            }),
            Event::Start(Tag::TableRow) => self.set_table(|t| t.current.clear()),
            Event::End(TagEnd::TableRow) => self.set_table(|t| {
                let fila = std::mem::take(&mut t.current);
                if !fila.is_empty() {
                    t.rows.push(fila);
                }
            }),
            Event::Start(Tag::TableCell) => self.set_table(|t| t.cell.clear()),
            Event::End(TagEnd::TableCell) => self.set_table(|t| {
                let celda = std::mem::take(&mut t.cell);
                t.current.push(celda);
            }),

            // ---- Links and images ----
            //
            // El único lugar donde se escribe una URL, y solo si `is_safe_url` dice
            // que sí. El `None` no es un caso raro: es el camino que toma
            // `javascript:`.
            Event::Start(Tag::Link { dest_url, .. }) => self.frames.push(Frame::Link {
                url: url_segura(&dest_url),
                spans: Vec::new(),
            }),
            Event::End(TagEnd::Link) => self.close(),

            Event::Start(Tag::Image { dest_url, .. }) => self.frames.push(Frame::Image {
                url: url_segura(&dest_url),
                spans: Vec::new(),
            }),
            Event::End(TagEnd::Image) => self.close(),

            // ---- Raw HTML from the document: never trusted ----
            //
            // El `HtmlBlock` se convierte en [`DocBlock::Opaque`] con el texto
            // legible del bloque. El `InlineHtml` **se tira**: el texto que hay
            // entre etiquetas llega como `Event::Text`, que ya entra por el
            // camino normal del párrafo. Medido con `examples/probe.rs`:
            // `<b>x</b>` produce `InlineHtml("<b>"), Text("x"), InlineHtml("</b>")`,
            // y `<div>x</div>` produce `Html("<div>x</div>\n")` **entero**, sin
            // ningún `Text` adentro. Por eso el caso de bloque necesita extraer
            // el texto y el inline no.
            Event::Start(Tag::HtmlBlock) => self.frames.push(Frame::Opaque {
                text: String::new(),
            }),
            Event::End(TagEnd::HtmlBlock) => self.close(),
            Event::Html(html) => {
                // Solo tiene sentido dentro de un `HtmlBlock` abierto: es la
                // única forma en que el parser produce un `Html` sin pareja. El
                // `if let` evita que un evento suelto se pierda y deja claro que
                // la extracción está atada al frame.
                if let Some(Frame::Opaque { text }) = self.frames.last_mut() {
                    text.push_str(&texto_legible_de_html(&html));
                }
            }
            Event::InlineHtml(_) => {}

            // ---- Inline content ----
            Event::Text(text) => {
                let marcas = self.marks_actuales();
                let texto = text.to_string();
                // Un span vacío no aporta nada y solo infla el árbol: un
                // `Event::Text` vacío aparece en los límites de un bloque de
                // código y en algunos saltos de línea suaves.
                if !texto.is_empty() {
                    self.push_span(Span::marked(texto, marcas));
                }
            }
            Event::Code(text) => {
                // El código inline **no** hereda la marca de énfasis que lo
                // rodea: `` `a` `` dentro de `**a**` es código, y el code span es
                // el elemento visible. Mezclarlos necesitaría una marca compuesta,
                // que es justo lo que este enum no tiene.
                let texto = text.to_string();
                if !texto.is_empty() {
                    self.push_span(Span::marked(texto, SpanMarks::Code));
                }
            }
            Event::Start(Tag::Strong) => self.marks.push(MarkFrame::Strong),
            Event::End(TagEnd::Strong) => {
                self.marks.pop();
            }
            Event::Start(Tag::Emphasis) => self.marks.push(MarkFrame::Emphasis),
            Event::End(TagEnd::Emphasis) => {
                self.marks.pop();
            }
            Event::Start(Tag::Strikethrough) => self.marks.push(MarkFrame::Strike),
            Event::End(TagEnd::Strikethrough) => {
                self.marks.pop();
            }

            Event::SoftBreak | Event::HardBreak => self.push_span(Span::plain(" ")),

            // Math. `parser_options()` no activa `ENABLE_MATH`, así que con las
            // opciones de la app estos eventos **no** llegan: la matemática se
            // resuelve por texto con `split_math`. Se enumeran igual porque el
            // `match` sobre `Event` sin `_` es lo que avisa si una versión futura
            // de `pulldown-cmark` agrega una variante.
            //
            // Y lo que hacen es **reponer los delimitadores**, porque el evento
            // los saca: `InlineMath("x^2")` es `$x^2$` para quien lee. Esa es la
            // forma que hace que la equivalencia con `render_markdown` se pueda
            // medir: los dos caminos ven el mismo texto.
            Event::InlineMath(tex) => {
                let texto = format!("${tex}$");
                self.push_span(Span::plain(texto));
            }
            Event::DisplayMath(tex) => {
                let texto = format!("$${tex}$$");
                self.push_span(Span::plain(texto));
            }

            Event::Rule => self.push(DocBlock::Rule),

            Event::FootnoteReference(name) => {
                self.push_span(Span::plain(format!("[^{name}]")));
            }
            Event::TaskListMarker(checked) => {
                self.push_span(Span::plain(if checked { "[x] " } else { "[ ] " }));
            }

            // Contenedores que el IR no modela. Se acepta el evento y se descarta
            // su contenido, en vez de devolver un error: el objetivo de un renderer
            // es que el documento sea legible, y perder un superíndice es mejor que
            // perder el párrafo.
            Event::Start(Tag::Superscript) | Event::End(TagEnd::Superscript) => {}
            Event::Start(Tag::Subscript) | Event::End(TagEnd::Subscript) => {}
            Event::Start(Tag::MetadataBlock(_)) | Event::End(TagEnd::MetadataBlock(_)) => {}

            // Listas de definiciones: `Options::ENABLE_DEFINITION_LIST` está
            // apagado en `parser_options`, así que el parser **nunca** emite
            // estos eventos con las opciones de la app. Los nonetheless se
            // enumeran, porque `match` sin `_` sobre `Event` es la red que
            // avisa cuando `pulldown-cmark` agrega una variante nueva — y ese
            // aviso tiene que aparecer en el `match`, no en un error de un crate de abajo.
            Event::Start(Tag::DefinitionList) => self.close(),
            Event::End(TagEnd::DefinitionList) => self.close(),
            Event::Start(Tag::DefinitionListTitle) => self.close(),
            Event::End(TagEnd::DefinitionListTitle) => self.close(),
            Event::Start(Tag::DefinitionListDefinition) => self.close(),
            Event::End(TagEnd::DefinitionListDefinition) => self.close(),
        }
    }

    /// Apply `f` to the open table, if there is one.
    ///
    /// Un helper y no `if let Some(Frame::Table(t)) = ..` repetido en cada brazo
    /// de tabla: los seis brazos de la tabla se leen mejor sin la misma
    /// desestructuración en los seis.
    ///
    /// Acepta un closure y no `&mut TableFrame` porque el borrow del `frames`
    ///——del `Builder`— y el del closure no se pueden partir: el closure se llama
    /// **mientras** está prestado el `frames`. Es el mismo motivo por el que
    /// `push` y `push_span` son métodos y no funciones libres.
    fn set_table(&mut self, f: impl FnOnce(&mut TableFrame)) {
        if let Some(Frame::Table(table)) = self.frames.last_mut() {
            f(table);
        }
    }
}

/// The URL, only if it passed the allowlist.
///
/// La función más corta del módulo y la que más seguridad sostiene. Es una
/// función y no un `if` en línea porque **el `match` de `event` la llama dos
/// veces** —link e imagen— y la segunda copia habría sido el lugar natural donde
/// alguien escribe `Some(dest_url.to_string())` por confianza.
fn url_segura(dest: &str) -> Option<String> {
    if is_safe_url(dest) {
        Some(dest.to_string())
    } else {
        None
    }
}

/// The visible text of a run of spans.
///
/// Concatena sin separador, a propósito: los `Span` de un enlace o de un alt de
/// imagen son trozos **contiguos** de la misma frase, y `concat` tiene que dar
/// exactamente el texto que el reader ve. Poner un espacio entre ellos
/// inventaría texto que el documento no tiene.
fn concat(spans: Vec<Span>) -> String {
    spans.into_iter().map(|span| span.text).collect()
}

/// El texto legible de un bloque de HTML crudo.
///
/// # Por qué esto no contradice "allowlist by construcción"
///
/// El módulo [`crate::markdown`] evita **sanitizar** HTML y construye la salida
/// desde una allowlist, y dice que hacerlo es frágil porque requiere que un
/// parser de HTML sea correcto. Esta función parece lo contrario, y la
/// diferencia vale una párrafo:
///
/// | | sanitizar | extraer texto |
/// |---|---|---|
/// | decide qué **marcado** se emite | sí | no |
/// | el resultado es marcado | sí | **nunca** |
/// | un error del extractor puede producir `<script>` | sí | no |
///
/// `Opaque.text` es un `String` que un renderer escribe en un `Text` de GPUI. No
/// hay ningún camino desde ahí hasta marcado: los colores y los tipos de letra
/// los pone el renderer, no el texto. Un error de esta función muestra una
/// palabra de más o de menos, y eso es un error de lectura, no de seguridad.
///
/// # Qué se tira y por qué
///
/// - El contenido de `<script>` y `<style>`: **no es texto legible**. Sin esto,
///   `<script>alert(1)</script>` mostraría `alert(1)` en el preview, que es
///   ruido y además asusta.
/// - Los comentarios `<!-- -->`: no son texto.
/// - Las etiquetas: `<tag attr="…">` desaparece entera, atributos incluidos. Es
///   lo que hace que `onclick=…` no sobreviva.
///
/// Lo que **queda** es lo que una persona escribiría dentro de las etiquetas: el
/// texto de un `<div>`, el de un `<summary>`.
///
/// # Lo que no descifra
///
/// No decodifica entidades HTML. `<div>a &amp; b</div>` deja `"a &amp; b"`, que es
/// lo que el lector ve en el archivo. Decodificar sería una segunda capa de
/// interpretación con su propia lista de casos; no vale la pena y sería un sitio
/// más donde equivocarse.
pub(crate) fn texto_legible_de_html(html: &str) -> String {
    #[derive(PartialEq, Eq, Clone, Copy)]
    enum Zona {
        /// Texto normal: se copia.
        Normal,
        /// Adentro de `<script>` o `<style>`: se descarta.
        SinTexto,
        /// Adentro de `<!-- -->`: se descarta.
        Comentario,
    }

    let chars: Vec<char> = html.chars().collect();
    let mut out = String::new();
    let mut zona = Zona::Normal;
    let mut i = 0;

    while i < chars.len() {
        if chars[i] != '<' {
            if zona == Zona::Normal {
                out.push(chars[i]);
            }
            i += 1;
            continue;
        }

        // El resto desde `<`, que es donde se decide qué hacer.
        let resto: String = chars[i..].iter().collect();

        // Comentario: se busca el cierre `-->`. Un `<!--` sin cerrar
        // se lleva todo lo que sigue, que es lo que hace cualquier lector también.
        //
        // La búsqueda va sobre el **recorte**, y esa es la parte con historia: la
        // primera versión hacía `resto[4..].find("-->").map(|p| p + 4)` y mezclaba
        // un índice relativo al recorte con uno relativo a `resto`, así que el
        // cursor quedaba antes del final del comentario y el `-->` de cierre
        // salía como texto suelto. `strip_prefix` separa las dos coordenadas de
        // una vez, y de paso es lo que pedía `clippy::manual_strip`.
        if let Some(cuerpo) = resto.strip_prefix("<!--") {
            match cuerpo.find("-->") {
                Some(fin) => {
                    zona = Zona::Normal;
                    i += 4 + largo_en_chars(&cuerpo[..fin + 3]);
                }
                None => {
                    zona = Zona::Comentario;
                    i = chars.len();
                }
            }
            continue;
        }

        // Etiqueta de cierre: `</…>`.
        if resto.starts_with("</") {
            if let Some(nombre) = nombre_de_etiqueta(resto.strip_prefix("</").unwrap_or("")) {
                if nombre == "script" || nombre == "style" {
                    zona = Zona::Normal;
                }
            }
            match resto.find('>') {
                Some(fin) => i += largo_en_chars(&resto[..fin + 1]),
                None => i = chars.len(),
            }
            continue;
        }

        // Etiqueta de apertura.
        let nombre = nombre_de_etiqueta(resto.strip_prefix('<').unwrap_or(""));
        if let Some(fin) = resto.find('>') {
            if nombre.as_deref() == Some("script") || nombre.as_deref() == Some("style") {
                zona = Zona::SinTexto;
            }
            i += largo_en_chars(&resto[..fin + 1]);
        } else {
            // `<` sin `>`: se lo trata como texto, porque un lector también
            // escribiría eso en el documento.
            out.push('<');
            i += 1;
        }
    }

    out
}

/// El nombre de la etiqueta justo después de `<` o `</`, en minúsculas.
///
/// `None` para `<!DOCTYPE`, `<3`, `< b>`: lo que sigue no es un nombre. Esa
/// distinción importa porque `<!` y `<3` son cosas que un documento puede
/// escribir y que **no** abren una etiqueta.
fn nombre_de_etiqueta(tras: &str) -> Option<String> {
    let nombre: String = tras
        .trim_start_matches(['/', '!', '?'])
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    if nombre.is_empty() {
        None
    } else {
        Some(nombre.to_ascii_lowercase())
    }
}

/// Cuántos **caracteres** hay en `trozo`.
///
/// # El error que esta función existe para no cometer
///
/// `str::find` devuelve un índice de **byte**, y un índice de byte no es un
/// número de caracteres si hay algo multibyte antes. La versión directa —`i += resto.find('>') + 1`—
/// se come un carácter de más en cuanto aparece un `á` dentro de un atributo, y
/// el bug se ve como **una palabra que desaparece del medio del texto**.
///
/// La primera versión de [`texto_legible_de_html`] hacía exactamente eso:
/// `<h1 id="título">` perdía la `T` de `Título`. Fijado en
/// `tests/doc_ir.rs::el_extractor_de_html_no_come_caracteres`.
fn largo_en_chars(trozo: &str) -> usize {
    trozo.chars().count()
}
