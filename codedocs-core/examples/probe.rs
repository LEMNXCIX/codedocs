//! Volcado de eventos crudos de `pulldown-cmark`.
//!
//! # Para qué existe
//!
//! Porque **medir antes de afirmar**.
//!
//! `pulldown-cmark` decide qué variantes de `Event` emite para cada construcción de
//! Markdown, y varias decisiones del IR dependen de detalles que no se deducen
//! leyendo el tipo:
//!
//! - `<div>x</div>` emite **un solo** `Html` con el bloque crudo adentro, sin
//!   ningún `Text`. Por eso [`DocBlock::Opaque`](codedocs_core::doc::DocBlock) tiene
//!   que extraer el texto.
//! - `<span>x</span>` emite `InlineHtml("<span>")`, `Text("x")`,
//!   `InlineHtml("</span>")`. El texto llega por el camino normal, así que el
//!   extractor no hace falta.
//! - Una lista **tensa** (`- a\n- b`) emite `Text` dentro de `Item` **sin**
//!   `Start(Paragraph)`; una **floja** (con línea en blanco) sí lo emite. De ahí
//!   el párrafo implícito.
//! - `What's` llega como `What’s`, porque `parser_options()` activa
//!   `ENABLE_SMART_PUNCTUATION`. De ahí que el slug del IR sea `what-s-new` y no
//!   `whats-new`.
//!
//! ```sh
//! cargo run -p codedocs-core --example probe
//! ```
//!
//! La lista de casos está **adentro del archivo**, y no es decorativa: cada entrada
//! corresponde a un comportamiento que los tests de `tests/doc_ir.rs` afirma. Si
//! cambiás un caso, cambias lo que el ejemplo demuestra.

use pulldown_cmark::{Options, Parser};

/// Los casos que los tests del IR dan por cierto.
///
/// Sacarlos de acá rompería el contrato: el ejemplo es la **evidencia** de que
/// `asegurar_parrafo` y `texto_legible_de_html` hacen falta, y un ejemplo que
/// demuestra otra cosa es peor que ninguno.
const CASOS: &[&str] = &[
    // Bloque de HTML: un solo `Html` crudo, sin `Text` adentro.
    "<div>nota importante</div>\n",
    "<img src=x onerror=alert(1)>\n",
    "<script>alert(1)</script>\n",
    // Inline: el texto llega como `Text`.
    "<span>inline</span>\n",
    "texto <b>negrita</b> mas\n",
    // Lista tensa: `Text` dentro de `Item` sin `Start(Paragraph)`.
    "- uno\n- dos\n- tres\n",
    // Lista floja: con `Start(Paragraph)`.
    "- uno\n\n- dos\n",
    // `Quote` siempre trae `Paragraph`.
    "> citado\n",
    // Lista ordenada: `Tag::List(Some(1))`.
    "1. uno\n",
];

/// Las mismas opciones que usa el crate.
///
/// Duplicadas y no importadas de `markdown::parser_options`, que es `pub(crate)`:
/// un ejemplo es un binario aparte y no puede ver lo privado del crate. Que
/// coincidan es lo que hace que la salida valga como evidencia.
fn opciones() -> Options {
    let mut o = Options::empty();
    o.insert(Options::ENABLE_TABLES);
    o.insert(Options::ENABLE_FOOTNOTES);
    o.insert(Options::ENABLE_STRIKETHROUGH);
    o.insert(Options::ENABLE_TASKLISTS);
    o.insert(Options::ENABLE_SMART_PUNCTUATION);
    o.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    o
}

fn main() {
    let o = opciones();
    for md in CASOS {
        println!("=== {md:?}");
        for evento in Parser::new_ext(md, o) {
            println!("   {evento:?}");
        }
    }
}
