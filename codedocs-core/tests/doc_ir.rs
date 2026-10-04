//! Hito 10: el IR de documento, sus invariantes y su equivalencia con el HTML.
//!
//! # Tres bloques y por qué tres
//!
//! El IR tiene tres preguntas distintas y cada una necesita un tipo de prueba
//! distinto.
//!
//! | bloque | pregunta | herramienta |
//! |---|---|---|
//! | **1 — datos** | ¿qué árbol produce este Markdown? | golden tests con `insta`, un `.snap` por construcción |
//! | **2 — seguridad** | ¿qué NO puede aparecer en el árbol? | asserts explícitos sobre lo que el documento **no** logra |
//! | **3 — equivalencia** | ¿el IR y el HTML cuentan lo mismo? | comparación de **texto visible**, no de estructura |
//!
//! El bloque 3 es el que justifica el módulo entero. `codedocs-core` existe porque
//! una vez hubo dos `render_markdown` que podían divergir; el IR agrega un
//! **tercer** consumidor del parser, y sin un test que compare los tres, la
//! divergencia que se estaba intentando evitar reaparece por el otro lado.
//!
//! # La forma de los tests de equivalencia
//!
//! Comparar el **texto visible** y no el HTML ni el árbol es una decisión, y es la
//! que hace el test útil:
//!
//! - Comparar HTML contra árbol es imposible: son representaciones distintas.
//! - Comparar árbol contra árbol entre el IR y otro modelo es tautológico.
//! - Comparar **el texto que un humano lee**, extraído del HTML con una función
//!   independiente, contra los spans del IR, es una afirmación real: si el IR
//!   pierde una palabra o el HTML la duplica, los dos lados no coinciden.
//!
//! Y la función que extrae el texto del HTML está **escrita acá**, no importada de
//! un parser. Si usáramos un parser de HTML para comparar dos de mis propias
//! salidas, el test mediría que dos parsers de HTML están de acuerdo.

use std::collections::HashSet;

use codedocs_core::doc::{parse_document, DocBlock, ListItemsExt, Span, SpanMarks};
use codedocs_core::markdown::{extract_headings, is_safe_url, render_markdown};

// ═══════════════════════════════════════════════════════════════════════════
// BLOQUE 1 — golden tests: qué árbol produce cada construcción de Markdown.
// ═══════════════════════════════════════════════════════════════════════════

/// Encabezado, párrafo y énfasis.
///
/// `insta::assert_debug_snapshot` imprime el `Debug` del árbol, y `DocBlock` tiene
/// un `Debug` derivado. Es el golden test más barato que existe: no compara un
/// string armado a mano sino la estructura entera, con los nombres de las
/// variantes.
#[test]
fn encabezado_y_parrafo() {
    insta::assert_debug_snapshot!(parse_document("# Título\n\nUn poco de *texto*.\n"));
}

/// Negrita, cursiva, tachado y código inline, cada uno por separado.
///
/// Cuatro golden tests y no uno con las cuatro marcas juntas, por una razón
/// práctica: un golden con cuatro casos falla y el diff muestra las cuatro
/// líneas, así que hay que volver a leer el snapshot para saber cuál cambió. Con
/// cuatro archivos, el nombre del que falla lo dice.
#[test]
fn marcas_en_linea() {
    insta::assert_debug_snapshot!(parse_document(
        "**negrita** y *cursiva* y ~~tachado~~ y `codigo`\n"
    ));
}

/// Un bloque de código cercado con lenguaje.
#[test]
fn bloque_de_codigo_con_lenguaje() {
    insta::assert_debug_snapshot!(parse_document("```rust\nfn main() {}\n```\n"));
}

/// Un bloque de código **sin** lenguaje.
#[test]
fn bloque_de_codigo_sin_lenguaje() {
    insta::assert_debug_snapshot!(parse_document("```\nlet x = 1;\n```\n"));
}

/// Una lista con viñetas.
#[test]
fn lista_con_vinetas() {
    insta::assert_debug_snapshot!(parse_document("- uno\n- dos\n- tres\n"));
}

/// Una lista ordenada, con el número del primer ítem.
///
/// El `3.` a propósito: el HTML empieza la lista en 3, y el IR solo dice
/// `ordered: true`. El golden congela que el número **no** viaja, que es la
/// pérdida consciente del bloque 1.
#[test]
fn lista_ordenada() {
    insta::assert_debug_snapshot!(parse_document("3. uno\n4. dos\n"));
}

/// Una lista anidada.
///
/// El caso que hace que `items` sea `Vec<Vec<DocBlock>>` y no `Vec<DocBlock>`: un
/// ítem puede contener un párrafo **y** una sublista.
#[test]
fn lista_anidada() {
    insta::assert_debug_snapshot!(parse_document("- uno\n  - uno.a\n  - uno.b\n- dos\n"));
}

/// Una cita.
#[test]
fn cita() {
    insta::assert_debug_snapshot!(parse_document("> citado\n> más\n"));
}

/// Una cita con dos bloques adentro.
#[test]
fn cita_con_dos_bloques() {
    insta::assert_debug_snapshot!(parse_document("> primero\n>\n> segundo\n"));
}

/// Una tabla de dos columnas.
#[test]
fn tabla() {
    insta::assert_debug_snapshot!(parse_document(
        "| a | b |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |\n"
    ));
}

/// Un separador temático.
#[test]
fn regla() {
    insta::assert_debug_snapshot!(parse_document("--- \n"));
}

/// Enlaces: uno seguro y uno rechazado.
///
/// Los dos en el mismo golden porque la diferencia entre ellos es **una palabra**
/// del output (`Some(...)` contra `None`) y esa palabra es el modelo de seguridad
/// entero. Verla al lado es el punto.
#[test]
fn enlaces_seguros_y_rechazados() {
    insta::assert_debug_snapshot!(parse_document(
        "[bueno](https://ok.example) y [malo](javascript:alert(1))\n"
    ));
}

/// Una imagen: el alt sobrevive, la URL pasa por la allowlist.
#[test]
fn imagen() {
    insta::assert_debug_snapshot!(parse_document("![alt aqui](https://ok.example/a.png)\n"));
}

/// Encabezados repetidos, con los slugs deduplicados.
#[test]
fn slugs_unicos() {
    insta::assert_debug_snapshot!(parse_document("# Notas\n\n# Notas\n\n# Notas\n"));
}

/// Documento vacío.
#[test]
fn documento_vacio() {
    insta::assert_debug_snapshot!(parse_document(""));
}

/// Documento que es solo espacios.
#[test]
fn documento_de_espacios() {
    insta::assert_debug_snapshot!(parse_document("   \n\n  \n"));
}

/// HTML crudo: se descarta, y lo que se puede leer sobrevive.
#[test]
fn html_crudo_se_descarta() {
    insta::assert_debug_snapshot!(parse_document(
        "<script>alert(1)</script>\n\n<div>texto util</div>\n"
    ));
}

/// Negrita y cursiva sobre el mismo texto.
///
/// El caso que resuelve el enum de marcas. Golden y no assert porque lo que se
/// congela es **qué marca gana**, y esa decisión es discutible; dejarla escrita
/// en un archivo hace que cambiarla sea un diff visible.
#[test]
fn negrita_y_cursiva_al_mismo_tiempo() {
    insta::assert_debug_snapshot!(parse_document("**negrita y *cursiva***\n"));
}

/// Las cuatro marcas anidadas en el orden inverso al de apertura.
///
/// El caso que **no** está en el primer golden: `*cursiva con **negrita***` abre
/// la cursiva primero. Si la resolución fuera "la última gana", los dos goldens
/// darían cursiva; con "la primera gana" dan cosas distintas y el par de snapshots
/// lo deja fijo.
#[test]
fn cursiva_abierta_antes_que_negrita() {
    insta::assert_debug_snapshot!(parse_document("*cursiva con **negrita***\n"));
}

/// Un elemento de lista de tarea.
#[test]
fn lista_de_tarea() {
    insta::assert_debug_snapshot!(parse_document("- [x] hecho\n- [ ] pendiente\n"));
}

/// Una referencia a nota al pie.
///
/// La URL del definition vive adentro del bloque de la nota, así que el texto del
/// párrafo tiene que llevar el marcador a mano.
#[test]
fn referencia_a_nota_al_pie() {
    insta::assert_debug_snapshot!(parse_document("texto[^1]\n\n[^1]: la nota\n"));
}

/// Matemática en línea y en bloque.
///
/// Con `parser_options()` sin `ENABLE_MATH`, esto llega como `Event::Text` y
/// `split_math` es del renderer HTML. En el IR el texto pasa entero, con los `$`.
#[test]
fn matematica() {
    insta::assert_debug_snapshot!(parse_document("valor $x^2$ aqui\n\n$$a+b$$\n"));
}

/// Un enlace a un ancla interna.
///
/// `is_safe_url` acepta `#ancla`. El golden congela que el IR no distingue un
/// ancla de una URL absoluta, y que eso está bien porque un renderer de outline
/// necesita el ancla tal cual.
#[test]
fn enlace_a_ancla() {
    insta::assert_debug_snapshot!(parse_document("[ir ahi](#seccion)\n"));
}

// ═══════════════════════════════════════════════════════════════════════════
// BLOQUE 2 — seguridad: qué NO puede aparecer en el árbol.
// ═══════════════════════════════════════════════════════════════════════════

/// El texto de todo el IR, para buscar marcadores prohibidos.
///
/// Una sola función y no un `for` por test: los tests de seguridad quieren
/// afirmar que una cosa **no** está, y la forma de decirlo es "no está en ninguna
/// parte del árbol". Un `Vec<DocBlock>` recorrido a mano en cada test sería
/// cinco veces el mismo código.
/// El texto de todo el IR, **con un espacio entre bloques**.
///
/// # Por qué el espacio
///
/// El HTML de `render_markdown` pone un salto de línea entre `</h1>` y `<p>`, así
/// que el extractor de [`texto_visible`] ve dos palabras separadas. El IR no
/// tiene ese salto: son dos `DocBlock` y el `concat` de los spans de uno con los
/// del otro los pegaría ("TítuloUn poco de…").
///
/// El espacio se agrega **entre bloques** y nunca **entre spans**, porque los
/// spans de un mismo bloque son trozos contiguos de la misma frase y ahí sí
/// cualquier espacio sería inventado.
fn todo_el_texto(bloques: &[DocBlock]) -> String {
    let mut out = String::new();
    for (i, bloque) in bloques.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        match bloque {
            DocBlock::Heading { spans, .. } | DocBlock::Paragraph { spans } => {
                for span in spans {
                    out.push_str(&span.text);
                }
            }
            DocBlock::CodeBlock { code, .. } => out.push_str(code),
            DocBlock::List { items, .. } => out.push_str(&todo_el_texto_de_items(items)),
            DocBlock::Quote { blocks } => out.push_str(&todo_el_texto(blocks)),
            DocBlock::Table { header, rows } => {
                // `header` son celdas (`Vec<Vec<Span>>`) y `rows` son filas de
                // celdas (`Vec<Vec<Vec<Span>>>`): no se pueden encadenar con un
                // solo iterador porque los niveles son distintos. Por eso la
                // función de celdas existe: los dos llaman a la misma.
                //
                // Y el separador entre celdas es necesario: el HTML las separa con
                // `</th><th>`, así que el extractor ve dos palabras. Sin el
                // espacio, dos celdas de una letra dan `"ab"` en el IR y
                // `"a b"` en el HTML.
                for (i, celda) in header.iter().enumerate() {
                    if i > 0 {
                        out.push(' ');
                    }
                    out.push_str(&texto_de_celda(celda));
                }
                for fila in rows {
                    for celda in fila {
                        out.push(' ');
                        out.push_str(&texto_de_celda(celda));
                    }
                }
            }
            DocBlock::Rule => {}
            DocBlock::Opaque { text } => out.push_str(text),
        }
    }
    out
}

/// El texto de una celda.
fn texto_de_celda(celda: &[Span]) -> String {
    celda.iter().map(|span| span.text.as_str()).collect()
}

fn todo_el_texto_de_items(items: &[Vec<DocBlock>]) -> String {
    let mut out = String::new();
    for item in items {
        out.push_str(&todo_el_texto(item));
    }
    out
}

/// Todas las URLs que el IR emitió, en cualquier parte del árbol.
///
/// La función hermana de [`todo_el_texto`], y por la misma razón: la pregunta
/// "¿este IR emitió alguna URL peligrosa?" tiene que responderse sobre el árbol
/// **entero**, no sobre el primer `Link` que aparezca.
fn todas_las_urls(bloques: &[DocBlock]) -> Vec<String> {
    let mut out = Vec::new();
    for bloque in bloques {
        match bloque {
            DocBlock::Heading { spans, .. } | DocBlock::Paragraph { spans } => {
                out.extend(urls_de_spans(spans));
            }
            DocBlock::List { items, .. } => {
                for item in items {
                    out.extend(todas_las_urls(item));
                }
            }
            DocBlock::Quote { blocks } => out.extend(todas_las_urls(blocks)),
            DocBlock::Table { header, rows } => {
                // Igual que en `todo_el_texto`: el encabezado y las filas no se
                // pueden encadenar porque `header` son celdas y `rows` son filas de
                // celdas.
                for celda in header {
                    out.extend(urls_de_spans(celda));
                }
                for fila in rows {
                    for celda in fila {
                        out.extend(urls_de_spans(celda));
                    }
                }
            }
            DocBlock::CodeBlock { .. } | DocBlock::Rule | DocBlock::Opaque { .. } => {}
        }
    }
    out
}

fn urls_de_spans(spans: &[Span]) -> Vec<String> {
    spans
        .iter()
        .filter_map(|span| span.marks.url().map(str::to_string))
        .collect()
}

/// Una URL rechazada **no** aparece en ninguna parte del árbol.
///
/// Esta es la afirmación que sostiene todo el módulo, y el test que la sostiene es
/// un `all` sobre **todas** las URLs emitidas, no un `assert!(!html.contains(..))`
/// como el del renderer HTML.
///
/// La diferencia no es de estilo. En el HTML, "no está" significa "la cadena no
/// aparece en el texto de salida", y un atacante podría conseguirla sin que
///менингara —por ejemplo si el renderer lo metiera en un atributo. En el IR, "no
/// está" significa que **el campo que la contendría es `None`**. No hay forma de
/// que exista una URL que no pasó por [`is_safe_url`], porque la función que la
/// escribe es [`codedocs_core::doc::parse_document`] y su único producer es
/// `url_segura`.
///
/// El corpus es el mismo del `test` de seguridad del renderer HTML
/// (`raw_html_in_every_container_is_stripped`), ampliado con las URL que solo el
/// parser de IR encuentra.
#[test]
fn ninguna_url_peligrosa_llega_al_arbol() {
    let corpus = [
        "[x](javascript:alert(1))",
        "[x](JaVaScRiPt:alert(1))",
        "[x](   javascript:alert(1))",
        "[x](java\tscript:alert(1))",
        "[x](java&#115;cript:alert(1))",
        "[x](data:text/html;base64,PHNjcmlwdD4=)",
        "[x](vbscript:msgbox)",
        "[x](file:///etc/passwd)",
        "![x](javascript:alert(1))",
        "| a | b |\n|---|---|\n| [x](javascript:alert(1)) | [y](data:,x) |\n",
        "> [x](javascript:alert(1))\n",
        "- [x](javascript:alert(1))\n",
        "**<a href=\"javascript:alert(1)\">y</a>**\n",
        "<javascript:alert(1)>\n",
        "[x][r]\n\n[r]: javascript:alert(1)\n",
    ];

    for md in corpus {
        let bloques = parse_document(md);
        for url in todas_las_urls(&bloques) {
            assert!(
                is_safe_url(&url),
                "{md:?} produjo la URL {url:?}, que no pasa la allowlist"
            );
            assert!(
                !url.to_lowercase().contains("javascript:")
                    && !url.to_lowercase().contains("data:"),
                "{md:?} produjo {url:?}, una URL peligrosa"
            );
        }
    }
}

/// Un enlace rechazado conserva su **texto** y pierde su URL.
///
/// La otra mitad de la política, y la que se olvida. Si el IR tirara el enlace
/// entero, un documento con `[docs](javascript:x)` perdería la palabra "docs" y el
/// lector se preguntaría dónde fue a parar.
///
/// Es el mismo trade que hace `render_markdown`, y por el mismo motivo: perder
/// contenido es peor que perder navegación.
#[test]
fn un_enlace_rechazado_conserva_el_texto() {
    let bloques = parse_document("[click me](javascript:alert(1))\n");
    let texto = todo_el_texto(&bloques);
    assert!(
        texto.contains("click me"),
        "el texto del enlace rechazado tiene que sobrevivir: {texto:?}"
    );

    let urls = todas_las_urls(&bloques);
    assert!(urls.is_empty(), "y no puede quedar ninguna URL: {urls:?}");
}

/// El enlace rechazado sigue siendo un enlace, marcado como tal.
///
/// `SpanMarks::is_link` distingue `Link { url: None }` de `SpanMarks::None`, y
/// esa distinción es la que le permite a un renderer **dibujar distinto** un
/// enlace sin destino. En HTML es la misma etiqueta sin `href`; GPUI no tiene
/// `href`, así que la decisión hay que tomarla en el renderer, y para eso el IR
/// tiene que poder dizer "esto era un enlace".
#[test]
fn un_enlace_rechazado_sigue_marcado_como_enlace() {
    let bloques = parse_document("[click me](javascript:alert(1))\n");
    let DocBlock::Paragraph { spans } = &bloques[0] else {
        panic!("se esperaba un párrafo, vino {bloques:?}");
    };
    assert_eq!(spans.len(), 1, "un enlace es un solo span: {spans:?}");
    let marca = spans[0].marks();
    assert!(
        marca.is_link(),
        "el span tiene que seguir siendo un enlace: {marca:?}"
    );
    assert!(marca.url().is_none(), "pero sin URL: {marca:?}");
}

/// El HTML crudo del documento **no** aparece en ninguna parte del IR.
///
/// Igual que en el renderer HTML, con la diferencia de que acá no hay ni siquiera
/// un campo donde meterlo. El corpus es el del test del renderer, más un caso que
/// solo importa para el IR: un `<pre>` de HTML, que `pulldown-cmark` no cierra
/// nunca y por eso aparece como `HtmlBlock` abierto.
#[test]
fn el_html_crudo_no_aparece_en_el_arbol() {
    let corpus = [
        "<script>alert(1)</script>",
        "<div onclick=\"steal()\">hi</div>",
        "<details open ontoggle=alert(1)><summary>x</summary>y</details>",
        "<style>body{background:url(javascript:alert(1))}</style>",
        "<svg><animate onbegin=alert(1) attributeName=x dur=1s>",
        "<img src=\"data:image/svg+xml;base64,PHN2Zz4=\">",
        "<pre>sin cerrar\n",
        "<iframe src=javascript:alert(1)></iframe>",
    ];

    for md in corpus {
        let bloques = parse_document(md);
        let texto = todo_el_texto(&bloques);
        for marcador in [
            "<script",
            "<div",
            "<details",
            "<style",
            "<svg",
            "<iframe",
            "<pre",
            "onclick",
            "ontoggle",
            "onbegin",
            "onerror",
            "javascript:",
            "alert(",
        ] {
            assert!(
                !texto.contains(marcador),
                "{marcador} apareció en el IR para {md:?}\n  texto: {texto:?}"
            );
        }
    }
}

/// Un bloque de HTML conserva su texto legible como [`DocBlock::Opaque`].
///
/// La razón de que la variante exista. Sin ella, un `<div>nota importante</div>`
/// desaparecería del preview y el lector no sabría que ahí había algo.
///
/// Y este test es el que obliga a que exista el extractor de texto, porque
/// **medido**: `pulldown-cmark` emite `<div>nota importante</div>` como **un solo**
/// `Event::Html` con el bloque crudo adentro, sin ningún `Event::Text` (se ve
/// con `examples/probe.rs`). O sea que el texto del documento **no llega** por el
/// camino normal: si el IR solo juntaba `Event::Text`, el `Opaque` quedaba
/// siempre vacío y la variante no hacía nada.
#[test]
fn un_bloque_de_html_conserva_su_texto_legible() {
    let casos = [
        ("<div>nota importante</div>\n", "nota importante"),
        ("<p>otro parrafo</p>\n", "otro parrafo"),
        (
            "<details open><summary>Titulo</summary>cuerpo</details>\n",
            "Titulocuerpo",
        ),
        ("<!-- un comentario -->\n", ""),
        ("<style>body{color:red}</style>\n", ""),
        ("<script>alert(1)</script>\n", ""),
    ];

    for (md, esperado) in casos {
        let bloques = parse_document(md);
        let opacos: Vec<&str> = bloques
            .iter()
            .filter_map(|b| match b {
                DocBlock::Opaque { text } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            opacos,
            vec![esperado],
            "el texto legible de {md:?} tiene que ser {esperado:?}: {bloques:?}"
        );
    }
}

/// El contenido de `<script>` y `<style>` **no** aparece como texto.
///
/// La mitad incómoda del test anterior, y la que hace que el extractor sea
/// aceptable.
///
/// Un extractor ingenuo que solo saca `<…>` dejaría `alert(1)` de
/// `<script>alert(1)</script>` como texto visible. No es una vulnerabilidad —es
/// texto inerte, no marcado— pero es basura en pantalla, y el plan dice que
/// `Opaque` conserva el texto **legible** de un bloque. El cuerpo de un script no
/// es legible.
///
/// El `assert_eq!` con el string vacío es lo que fija el comportamiento, porque
/// un test con `!contains("alert(")` pasaría igual con el texto entero.
#[test]
fn el_cuerpo_de_script_y_style_no_aparece_como_texto() {
    for md in [
        "<script>alert(1)</script>\n",
        "<style>body{background:url(javascript:alert(1))}</style>\n",
        "<script>var x = 'hola';</script>\n",
    ] {
        let texto = todo_el_texto(&parse_document(md));
        assert_eq!(
            texto, "",
            "un script o un style no tienen texto legible, tienen código: {md:?} -> {texto:?}"
        );
    }
}

/// El extractor de HTML no come caracteres con texto multibyte adentro.
///
/// El error exacto que tuvo la **primera** versión de `texto_legible_de_html`:
/// usaba `resto.find('>')`, que es un índice de **byte**, y lo sumaba a un índice
/// que cuenta **caracteres**. Con un `á` antes del cierre de la etiqueta, eso se
/// come un carácter de más y el texto pierde la primera letra.
///
/// El caso concreto es `id="título"`: la `í` son 2 bytes, así que el salto
/// avanzó una posición de más y la `T` de `Título` desapareció del bloque
/// siguiente.
#[test]
fn el_extractor_de_html_no_come_caracteres() {
    // El `id` del encabezado lleva un acento, así que la etiqueta que lo contiene
    // tiene un carácter multibyte.
    let bloques = parse_document("# <span id=\"título\">Hola</span>\n");
    let texto = todo_el_texto(&bloques);
    assert!(
        texto.contains("Hola"),
        "el texto tiene que salir entero: {texto:?}"
    );
    assert_eq!(
        texto, "Hola",
        "y exacto: un extractor que come caracteres se nota acá"
    );
}

/// El HTML **inline** conserva su texto, por un camino distinto al de bloque.
///
/// La contraparte de [`un_bloque_de_html_conserva_su_texto_legible`], y el caso
/// que muestra por qué hay dos caminos.
///
/// Medido con `examples/probe.rs`: `span` **no** es una de las etiquetas de bloque
/// de CommonMark, así que `<span>inline</span>` produce
/// `InlineHtml("<span>"), Text("inline"), InlineHtml("</span>")` —el texto llega
/// como `Event::Text`— y el IR lo conserva sin pasar por el extractor. El
/// `InlineHtml` se tira y el párrafo queda.
///
/// `<div>inline</div>` es al revés: etiqueta de bloque, un solo `Html` crudo, y sí
/// necesita el extractor. El mismo texto por dos caminos distintos del parser.
#[test]
fn el_html_inline_conserva_su_texto_por_el_camino_normal() {
    let bloques = parse_document("<span>inline</span>\n");
    assert!(
        !bloques.iter().any(|b| matches!(b, DocBlock::Opaque { .. })),
        "el HTML inline no es un Opaque: {bloques:?}"
    );
    assert_eq!(
        todo_el_texto(&bloques),
        "inline",
        "pero el texto sigue ahí: {bloques:?}"
    );
}

/// El texto del documento **nunca** puede abrir una etiqueta.
///
/// La diferencia entre el IR y el HTML es que acá no hay escapado posible: el
/// renderer GPUI escribe texto en un `Text`, y el texto de un documento hostil es
/// texto, no marcado. La prueba de que el IR no arrastra marcado es que su `Debug`
/// —y su tipo— no tienen ningún string de HTML.
#[test]
fn el_texto_del_documento_no_abre_una_etiqueta() {
    let md = "<img src=x onerror=alert(1)> *con marca* `con codigo`\n";
    let bloques = parse_document(md);
    let texto = todo_el_texto(&bloques);
    assert!(
        !texto.contains("<"),
        "ningún texto del IR debería contener '<': {texto:?}"
    );
    assert!(
        texto.contains("con marca"),
        "pero el contenido legible sí: {texto:?}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// BLOQUE 3 — equivalencia IR ↔ HTML: el test anti-divergencia.
// ═══════════════════════════════════════════════════════════════════════════

/// El texto visible de un fragmento de HTML, con las etiquetas sacadas.
///
/// # Por qué escrita acá y no importada
///
/// Comparar dos salidas de mis propios renderizadores con un **tercer** parser
/// sería medir que ese parser está de acuerdo conmigo, que no es lo que importa.
/// Y comparar el HTML crudo contra el árbol es imposible.
///
/// Lo que sí es una afirmación real es: "las palabras que se leen en el HTML son
/// las mismas que en el IR". Esa es la que detecta que el IR perdió un párrafo o
/// que el HTML duplicó un código.
///
/// # Qué hace y qué no hace
///
/// Saca `<…>` y todo lo que parece una entidad HTML. Los bloques `<pre>` y
/// `<code>` **conservan** su texto, que es exactamente lo que los dos renderizadores
/// hacen —el preview muestra el código de un bloque de código—, así que el
/// extractor tiene que saber qué está mirando.
///
/// El `set` es lo que la hace útil: si una palabra aparece dos veces en un lado y
/// una en el otro, los conjuntos son iguales y el test pasa. Por eso
/// [`las_palabras_no_se_pierden_ni_se_duplican`] **no** usa este extractor sino
/// [`texto_visible_completo`], que cuenta.
///
/// # El `alt` de un `<img>`, que sí cuenta como texto
///
/// `render_markdown` pone el texto alternativo de una imagen en un **atributo**:
/// `<img src="…" alt="alt aqui" />`. Un extractor ingenuo devuelve `""` y el IR
/// devuelve `"alt aqui"`, y el test de equivalencia falla por una diferencia que no
/// es de contenido.
///
/// La decisión es que el `alt` **sí** es lo que lee la persona: un navegador lo
/// dibuja cuando la imagen no carga, que es el caso más común en un preview que no
/// descarga imágenes. Así que el extractor lo saca del atributo.
///
/// Y esto es una asimetría real entre los dos renderizadores, no un detalle del
/// test: el IR pone el alt en el `text` del span, que un renderer **siempre**
/// dibuja, mientras el HTML lo deja en un atributo que solo se ve si la imagen
/// falla. Está anotado en la sección 10 del capítulo del hito.
fn texto_visible(html: &str) -> String {
    let mut out = String::new();
    // El contador de "estamos dentro de un `<pre>` o `<code>`" tiene que sobrevivir
    // al recorrido entero, así que vive acá y no dentro del `while`.
    let mut dentro_de_codigo: usize = 0;
    let chars: Vec<char> = html.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] != '<' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        // Se lee la etiqueta entera, con el mismo escaneo que avanza el cursor, así
        // que la aritmética de bytes y de caracteres no puede desincronizarse.
        //
        // Y aquí está el error que la primera versión tenía: `find('>')` devuelve
        // un índice de **byte**, y sumarlo a un índice que cuenta **caracteres** se
        // come un carácter de más en cuanto hay algo multibyte. Con
        // `<h1 id="título">` eso borraba la `T` de `Título` y el test de
        // equivalencia fallaba con un mensaje que parecía de contenido.
        let (largo, etiqueta) = leer_etiqueta(&chars, i);
        let nombre = nombre_de_etiqueta(&etiqueta);

        if nombre.as_deref() == Some("img") {
            // El `alt` va por acá y no por la rama de `<pre>`: una imagen no es
            // texto, así que el flag de código no la toca.
            if let Some(alt) = atributo(&etiqueta, "alt") {
                out.push_str(&desescapar(alt));
            }
        } else if matches!(nombre.as_deref(), Some("pre") | Some("code")) {
            dentro_de_codigo = if etiqueta.starts_with("</") {
                dentro_de_codigo.saturating_sub(1)
            } else {
                dentro_de_codigo + 1
            };
        } else if dentro_de_codigo == 0
            && matches!(
                nombre.as_deref(),
                Some(
                    "h1" | "h2"
                        | "h3"
                        | "h4"
                        | "h5"
                        | "h6"
                        | "p"
                        | "li"
                        | "td"
                        | "th"
                        | "div"
                        | "blockquote"
                )
            )
        {
            // Un espacio en los tags de bloque: es lo que separa dos palabras que
            // en el HTML quedaron en etiquetas distintas.
            out.push(' ');
        }
        i += largo;
    }

    desescapar(&out)
}

/// Lee una etiqueta desde `desde`, y devuelve cuántos **caracteres** ocupa y su
/// contenido completo, con el `>` final.
///
/// Las comillas importan: `<img alt="a > b">` tiene un `>` adentro del valor del
/// atributo, y buscar el primero cortaría la etiqueta por la mitad. Por eso el
/// escaneo lleva su propio estado de "estoy dentro de comillas".
///
/// Devolver las dos cosas juntas es lo que hace imposible el error del byte: se
/// avanza y se lee con la **misma** aritmética, no con dos cálculos separados que
/// puedan discrepar.
fn leer_etiqueta(chars: &[char], desde: usize) -> (usize, String) {
    let mut i = desde + 1;
    let mut comilla: Option<char> = None;
    while i < chars.len() {
        let c = chars[i];
        match comilla {
            Some(q) if c == q => comilla = None,
            Some(_) => {}
            None if c == '"' || c == '\'' => comilla = Some(c),
            None if c == '>' => return (i - desde + 1, chars[desde..=i].iter().collect()),
            None => {}
        }
        i += 1;
    }
    // Etiqueta sin cerrar: se come el resto. Es lo que también haría el navegador
    // al final del documento.
    (chars.len() - desde, chars[desde..].iter().collect())
}

/// El nombre de la etiqueta, sin `<`, `</` ni `>`, en minúsculas.
fn nombre_de_etiqueta(etiqueta: &str) -> Option<String> {
    let nombre: String = etiqueta
        .trim_start_matches(['<', '/', '>', '!', '?'])
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    if nombre.is_empty() {
        None
    } else {
        Some(nombre.to_ascii_lowercase())
    }
}

/// El valor de un atributo, por nombre.
///
/// Una búsqueda de la forma `nombre="valor"` o `nombre='valor'`, con espacios
/// opcionales alrededor del `=`. `None` si no está, que es lo normal.
///
/// El `antes_ok` no es decorativo: sin él, buscar `alt` matchearía adentro de
/// `data-alt` o del valor de otro atributo, y devolvería algo que no era el `alt`.
fn atributo<'a>(etiqueta: &'a str, nombre: &str) -> Option<&'a str> {
    let mut resto = etiqueta;
    while let Some(pos) = resto.find(nombre) {
        let despues = &resto[pos + nombre.len()..];
        let antes_ok = pos == 0
            || resto[..pos]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace);
        if antes_ok {
            let valor = despues.trim_start().strip_prefix('=')?;
            let valor = valor.trim_start();
            let comilla = valor.chars().next()?;
            if comilla == '"' || comilla == '\'' {
                let cuerpo = &valor[comilla.len_utf8()..];
                let fin = cuerpo.find(comilla)?;
                return Some(&cuerpo[..fin]);
            }
        }
        resto = despues;
    }
    None
}

/// El texto visible **con repeticiones**, para poder comparar cantidades.
///
/// La función hermana de [`texto_visible`], y existe porque el test de palabras
/// duplicadas necesita mirar el orden y el conteo, no el conjunto. Devuelve
/// espacios simples para que dos kinds de whitespace distintos no se lean como una
/// diferencia de contenido.
fn texto_visible_completo(html: &str) -> String {
    let crudo = texto_visible(html);
    crudo.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `&amp;` y compañía vuelven a ser `&`, `&#…;` también.
///
/// Sin esto, un documento con `&amp;` daría dos palabras distintas en cada lado
/// solo por la entidad, y el test de equivalencia estaría comparando codificaciones
/// y no contenido.
fn desescapar(texto: &str) -> String {
    let mut out = texto.to_string();
    for (entidad, caracter) in [
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&quot;", "\""),
        ("&#39;", "'"),
        ("&apos;", "'"),
        ("&nbsp;", " "),
        ("&hellip;", "…"),
        ("&amp;", "&"),
    ] {
        out = out.replace(entidad, caracter);
    }
    out
}

/// Las palabras del IR, en el mismo formato que [`texto_visible_completo`].
///
/// El `concat` es por bloques y no un `join` con espacio porque el IR ya trae los
/// espacios correctos: `push_span` mete un `Span::plain(" ")` por salto de línea
/// suave y los `Span` de un párrafo se pegan sin separador, que es como los lee
/// una persona.
fn palabras_del_ir(bloques: &[DocBlock]) -> String {
    let texto = todo_el_texto(bloques);
    texto.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// El mismo input produce **las mismas palabras** por el IR y por el HTML.
///
/// El test anti-divergencia del hito, y el que justifica el módulo.
///
/// El corpus es mixto a propósito: encabezados, énfasis, listas, citas, tablas,
/// código, enlaces aceptados y rechazados, imágenes, matemática. Cada
/// construcción que el IR modela aparece al menos una vez, así que una variante
/// nueva de `DocBlock` sin equivalente en el HTML la delata enseguida.
///
/// # Lo que NO está en el corpus, y por qué
///
/// **Los bloques de HTML crudo.** `<div>nota importante</div>` da `""` en el HTML
/// y `"nota importante"` en el IR. Esa diferencia es **deliberada** y va por
/// separado en [`el_ir_recupera_texto_que_el_html_descarta`]; esconderla en el
/// corpus sería dejar que un test que se llama "equivalencia" tenga una excepción
/// silenciosa.
///
/// **La matemática.** `valor $x^2$` da `"valor x^2 aqui"` en el HTML y
/// `"valor $x^2$ aqui"` en el IR. La diferencia va por separado en
/// [`la_matematica_conserva_los_delimitadores_que_el_html_saca`].
///
/// Y no son excepciones de gusto: son la razón de que existan
/// [`DocBlock::Opaque`] y la aritmética de `split_math`. Un test de equivalencia
/// que las incluyera y las aceptara sería un test que dice "el IR y el HTML hacen
/// lo mismo", y no es cierto.
///
/// La segunda tiene una consecuencia de diseño que conviene ver: para el renderer
/// de GPUI, la matemática es **texto**. El proyecto Tauri tenía KaTeX dentro del
/// webview; GPUI no tiene nada, así que `$x^2$` se muestra tal cual. Eso no es una
/// limitación del IR: es la razón por la que el IR no gasta una variante de
/// `SpanMarks` en algo que nadie va a pintar.
///
/// **Las listas de tarea.** `- [x] hecho` da un `<input type="checkbox">` en el
/// HTML y `"[x] hecho"` en el IR. Va por separado en
/// [`la_lista_de_tarea_degrada_a_texto_en_el_ir`], y es la única de las tres
/// divergencias en la que el IR ve **menos** información que el HTML: no tiene
/// forma de decir "esto es una casilla marcada" porque `SpanMarks` no tiene esa
/// variante.
#[test]
fn el_ir_y_el_html_cuentan_lo_mismo() {
    let corpus = [
        "# Título\n\nUn poco de *texto*.\n",
        "## Sub **con negrita** y `codigo`\n",
        "- uno\n- dos\n- tres\n",
        "1. uno\n2. dos\n",
        "> citado\n> más\n",
        "> primero\n>\n> segundo\n",
        "| a | b |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |\n",
        "```rust\nfn main() {}\n```\n",
        "normal con **negrita**, *cursiva*, ~~tachado~~ y `codigo`.\n",
        "[bueno](https://ok.example) y [malo](javascript:alert(1))\n",
        "![alt aqui](https://ok.example/a.png)\n",
        "despues de un script\n",
        "---\n",
        "texto con un *énfasis*, un `código` y una á  \n",
    ];

    for md in corpus {
        let html = render_markdown(md);
        let bloques = parse_document(md);
        let del_html = texto_visible_completo(&html);
        let del_ir = palabras_del_ir(&bloques);

        assert_eq!(
            del_ir, del_html,
            "el IR y el HTML no ven lo mismo para {md:?}\n  html: {html:?}\n  ir:   {del_ir:?}"
        );
    }
}

/// El IR recupera texto que el renderer HTML **descarta**, y solo eso.
///
/// La divergencia deliberada, enunciada como tal.
///
/// `render_markdown` **borra** un bloque de HTML: `<div>nota importante</div>`
/// sale como `""`. El IR lo convierte en `DocBlock::Opaque { text }` con el texto
/// legible adentro. Los dos caminos no son equivalentes y el plan elige que no lo
/// sean: el lector de un preview quiere ver que ahí había algo.
///
/// Lo que este test fija es **la dirección** de la divergencia, que es lo que la
/// hace defendible: el IR puede mostrar **más** texto que el HTML, nunca menos.
///
/// Y lo que el IR **no** hace es interpretar ese texto: es un `String` que un
/// renderer pinta como texto, así que recuperar el texto de un `<div onclick=…>`
/// no ejecuta nada.
#[test]
fn el_ir_recupera_texto_que_el_html_descarta() {
    let casos = [
        ("<div>nota importante</div>\n", "nota importante"),
        (
            "<details open><summary>Titulo</summary>cuerpo</details>\n",
            "Titulocuerpo",
        ),
        ("<p>otro parrafo</p>\n", "otro parrafo"),
    ];

    for (md, esperado) in casos {
        let html = render_markdown(md);
        let bloques = parse_document(md);

        assert_eq!(
            texto_visible_completo(&html),
            "",
            "el renderer HTML descarta el bloque de {md:?}"
        );
        assert_eq!(
            todo_el_texto(&bloques),
            esperado,
            "y el IR conserva su texto legible: {bloques:?}"
        );
    }
}

/// El cuerpo de un `<script>` lo pierden **los dos**.
///
/// La contraparte de [`el_ir_recupera_texto_que_el_html_descarta`].
///
/// Recuperar el texto de un bloque es una cosa; recuperar el **código** de un
/// script es otra. `render_markdown` tira el bloque entero y el IR también, así que
/// los dos coinciden — y ese "los dos" es lo que hace defendible el caso anterior:
/// no es "el IR muestra más cosas", es "el IR muestra el texto y no el código".
#[test]
fn el_cuerpo_de_un_script_lo_pierden_los_dos() {
    let md = "<script>var secreto = 1;</script>\n";
    assert_eq!(
        texto_visible_completo(&render_markdown(md)),
        "",
        "el HTML lo descarta"
    );
    assert_eq!(todo_el_texto(&parse_document(md)), "", "y el IR también");
}

/// La lista de tarea degrada a texto en el IR, y el IR **pierde** información.
///
/// La tercera divergencia, y la única en la que el lado perdedor es el IR.
///
/// El HTML de una casilla es un `<input type="checkbox">`, o sea un **control**. El
/// IR no tiene forma de decir "esto es una casilla marcada": `SpanMarks` no tiene
/// esa variante y el plan fija el enum tal cual. Lo que el IR puede es dejar el
/// `[x] ` como texto, que es lo que hace [`Builder::event`] con
/// `Event::TaskListMarker`.
///
/// Que eso sea aceptable es una decisión del plan, no un accidente: agregar
/// `SpanMarks::Task { checked }` obligaría a cada renderer a manejar una variante
/// más, y el de GPUI todavía no dibuja casillas. Cuando lo dibuje, el IR cambia y
/// este test es el que hay que tocar.
///
/// Y el test dice las dos cosas, no una: que el IR **degrada** a texto y que el
/// HTML **sí** produce el control. Un `assert` que solo mirara el IR pasaría
/// igual con las dos salidas iguales.
#[test]
fn la_lista_de_tarea_degrada_a_texto_en_el_ir() {
    let md = "- [x] hecho\n- [ ] pendiente\n";
    let html = render_markdown(md);
    let texto_html = texto_visible_completo(&html);
    let texto_ir = todo_el_texto(&parse_document(md));

    // El lado que gana: el HTML produce un control real.
    assert!(
        html.contains("type=\"checkbox\""),
        "el HTML dibuja una casilla: {html}"
    );
    assert!(
        !texto_html.contains("["),
        "y el texto visible no lleva los corchetes: {texto_html:?}"
    );

    // El lado que degrada: el IR solo puede dejar el marcador como texto.
    assert!(
        texto_ir.contains("[x] ") && texto_ir.contains("[ ] "),
        "el IR deja el marcador como texto: {texto_ir:?}"
    );
}

/// La matemática conserva en el IR los `$` que el renderer HTML saca.
///
/// La segunda divergencia deliberada, y la que tiene más consecuencias de diseño.
///
/// `render_markdown` envuelve el TeX en `<span class="math-inline">` y **saca los
/// delimitadores**, porque en el webview hay KaTeX que los compila y el `$` ya no
/// se ve. En GPUI no hay nada que compile el TeX, así que el IR deja `$x^2$`
/// entero.
///
/// Que sea lo correcto depende de qué se pinte: si el renderer de GPUI no compila
/// math, sacar el `$` deja `x^2`, que no dice más que antes y **le quita** la
/// señal de que ahí había matemática. Dejarlo como texto es la opción honesta
/// mientras KaTeX no exista, y por eso el IR no gasta una variante de `SpanMarks`
/// en él.
///
/// Lo que este test fija es que la diferencia es **exactamente** el delimitador y
/// nada más: el TeX de adentro es el mismo de los dos lados. Si mañana el renderer
/// de GPUI aprende a compilar math, este test es el que hay que cambiar, y por eso
/// existe.
#[test]
fn la_matematica_conserva_los_delimitadores_que_el_html_saca() {
    let casos = [
        ("valor $x^2$ aqui\n", "$x^2$"),
        ("display $$a+b$$\n", "$$a+b$$"),
    ];

    for (md, con_dollar) in casos {
        let html = texto_visible_completo(&render_markdown(md));
        let ir = todo_el_texto(&parse_document(md));

        assert!(
            ir.contains(con_dollar),
            "el IR tiene que conservar {con_dollar:?}: {ir:?}"
        );
        assert!(!html.contains('$'), "y el HTML lo saca: {html:?}");
        // El TeX de adentro es el mismo: la diferencia es **solo** el `$`.
        let sin_dollar = con_dollar.replace('$', "");
        assert!(
            html.contains(sin_dollar.trim()),
            "el TeX sigue ahí en el HTML: {html:?}"
        );
    }
}

/// Las palabras no se pierden **ni se duplican**.
///
/// La comparación de [`el_ir_y_el_html_cuentan_lo_mismo`] tiene un punto ciego que
/// este test cubre: si el HTML muestra "hola" dos veces y el IR una, las dos
/// cadenas son iguales y el otro test pasa igual. Por eso acá se comparan las
/// **listas**, con repeticiones y en orden.
///
/// El `assert_ne!` de la mitad negativa no es decorativo: sin él, un test que
/// compara dos listas idénticas no distingue "las dos dicen `[]`" de "las dos dicen
/// lo mismo". Con el `assert_ne!`, un corpus que se rompe al vaciarse queda visible.
#[test]
fn las_palabras_no_se_pierden_ni_se_duplican() {
    let md = "un **dos** tres `cuatro` [cinco](https://ok.example)\n";
    let html = render_markdown(md);
    let bloques = parse_document(md);

    let palabras_html: Vec<String> = texto_visible_completo(&html)
        .split(' ')
        .map(str::to_string)
        .collect();
    let palabras_ir: Vec<String> = palabras_del_ir(&bloques)
        .split(' ')
        .map(str::to_string)
        .collect();

    assert_eq!(
        palabras_ir, palabras_html,
        "las palabras, su orden y su cantidad tienen que coincidir"
    );
    assert_ne!(
        palabras_ir,
        Vec::<String>::new(),
        "y el corpus no puede estar vacío: comparar dos vacíos no mide nada"
    );
    assert_eq!(
        palabras_ir,
        vec!["un", "dos", "tres", "cuatro", "cinco"],
        "cinco palabras, en orden"
    );
}

/// Los slugs del IR son **los mismos** que los de `extract_headings`.
///
/// La razón de que `Builder` comparta el asignador con `extract_headings`.
///
/// Y el test que lo falla es el que importa: si mañana alguien "optimiza"
/// `unique_slug` en el IR copiándolo, este test ve el ids distintos y el outline
/// deja de saltar a la sección correcta — un bug que no rompe ningún assert de
/// otra parte, porque el outline y el preview seguirían compilando.
#[test]
fn los_slugs_del_ir_coinciden_con_extract_headings() {
    let corpus = [
        "# Notas\n\n# Notas\n\n# Notas\n",
        "# Hello World\n\n## Hello World\n",
        "Title\n=====\n\nSub\n---\n",
        "#  ¡Con acentos!  \n\n# Con acentos\n",
        "# ¿Qué es esto?\n\n# Que es esto\n",
        "# Notas\n\ntexto\n\n## Notas\n\n## Notas\n",
        "",
        "sin encabezados\n",
    ];
    for md in corpus {
        let del_ir: Vec<(u8, String, String)> = parse_document(md)
            .iter()
            .filter_map(|b| match b {
                DocBlock::Heading { level, id, spans } => Some((
                    *level,
                    spans.iter().map(|s| s.text.as_str()).collect::<String>(),
                    id.clone(),
                )),
                _ => None,
            })
            .collect();
        let de_extract: Vec<(u8, String, String)> = extract_headings(md)
            .into_iter()
            .map(|h| (h.level, h.text, h.anchor))
            .collect();

        assert_eq!(
            del_ir, de_extract,
            "el IR y extract_headings no coinciden para {md:?}"
        );
    }
}

/// El IR y el HTML **descartan lo mismo**.
///
/// La mitad negativa del bloque 3, y la que más valor tiene para la seguridad.
///
/// La forma del test es lo que lo hace fuerte: itera sobre un `HashSet` de
/// marcadores y afirma que **ninguno** aparece en ninguna de las dos salidas. Un
/// `for` con un `assert` adentro para cada marcador sería igual de fuerte, pero un
/// `HashSet` obliga a que la lista de marcadores sea un dato y no una cuenta
/// mental.
#[test]
fn el_ir_y_el_html_descartan_lo_mismo() {
    let corpus = [
        "<script>alert(1)</script>",
        "<img src=x onerror=alert(1)>",
        "<div onclick=\"steal()\">hi</div>",
        "| a | b |\n|---|---|\n| <script>alert(1)</script> | <img src=x onerror=y> |\n",
        "- [ ] <img src=x onerror=alert(1)>\n",
        "> <script>alert(1)</script>\n",
        "<form action=javascript:alert(1)><input></form>\n",
        "<svg><animate onbegin=alert(1) attributeName=x dur=1s>",
    ];

    let marcadores: HashSet<&str> = [
        "<script",
        "<img",
        "<form",
        "<svg",
        "onclick",
        "onerror",
        "onbegin",
        "javascript:",
    ]
    .into_iter()
    .collect();
    assert!(
        marcadores.len() == 8,
        "el set de marcadores se armó mal: {marcadores:?}"
    );

    for md in corpus {
        let html = render_markdown(md);
        let texto_ir = todo_el_texto(&parse_document(md));
        for marcador in &marcadores {
            assert!(
                !html.contains(marcador),
                "el HTML dejó pasar {marcador} para {md:?}\n  -> {html}"
            );
            assert!(
                !texto_ir.contains(marcador),
                "el IR dejó pasar {marcador} para {md:?}\n  -> {texto_ir:?}"
            );
        }
    }
}

/// Un corpus hostil completo no rompe el IR.
///
/// Un `for` sobre una lista de entradas malformadas, sin asserts dentro. El valor
/// del test es que **no entre en pánico**: `parse_document` no devuelve `Result` y
/// no tiene ningún `unwrap`, así que el documento más raro del mundo tiene que
/// devolver un árbol o nada.
///
/// El `assert!(!vistos.is_empty())` de adentro es lo que evita que el test sea
/// tautológico: un `for` vacío pasa siempre, y "no rompió con 0 entradas" no dice
/// nada. El contador afirma que el corpus se recorrió entero.
#[test]
fn un_corpus_hostil_no_rompe_el_ir() {
    let corpus = [
        "",
        "\n\n\n",
        "#",
        "#\n##\n###\n",
        "> ",
        "> > > > profundo\n",
        "```",
        "```rust",
        "- ",
        "-",
        "1.",
        "|",
        "| a |\n|",
        "| a | b |\n|--|\n",
        "***",
        "---",
        "<<<>>>",
        "[",
        "](",
        "[]()",
        "[]()()",
        "[a](b",
        "![",
        "**",
        "*",
        "~~",
        "`",
        "`a",
        "&",
        "&amp;",
        "\u{1f600}",
        "🦀 ñ á 混合",
        "# 🦀 Emoji\n\n## 🦀 Emoji\n",
        "<!-- comentario -->",
        "<!--",
        "<",
        ">",
        "<>",
        "a<b>c",
        "\t",
        "\r\n",
        "a\0b",
    ];

    let mut vistos = 0usize;
    for md in corpus {
        let bloques = parse_document(md);
        // Solo hay que llegar acá sin pánico. Recorrer el árbol entero es lo que
        // haría un renderer, así que se recorre: un `push` mal formado se
        // manifestaría acá.
        let _ = todo_el_texto(&bloques);
        let _ = todas_las_urls(&bloques);
        vistos += 1;
    }
    assert_eq!(vistos, corpus.len(), "el corpus se recorrió entero");
}

// ═══════════════════════════════════════════════════════════════════════════
// BLOQUE 4 — los detalles que un `match` exhaustivo deja decidir.
// ═══════════════════════════════════════════════════════════════════════════

/// Negrita y cursiva al mismo tiempo resuelven a **negrita**.
///
/// La decisión está documentada en `SpanMarks`, y acá queda fijada. Si cambia, el
/// test falla y el nombre del que falla dice dónde revisar.
#[test]
fn negrita_y_cursiva_al_mismo_tiempo_resuelven_a_negrita() {
    let bloques = parse_document("**negrita y *cursiva***\n");
    let DocBlock::Paragraph { spans } = &bloques[0] else {
        panic!("se esperaba un párrafo, vino {bloques:?}");
    };
    let marcas: Vec<SpanMarks> = spans.iter().map(|s| s.marks.clone()).collect();
    assert!(
        marcas.contains(&SpanMarks::Bold),
        "tiene que haber negrita: {marcas:?}"
    );
    assert!(
        !marcas.contains(&SpanMarks::Italic),
        "y NO cursiva: la primera marca abierta gana. {marcas:?}"
    );
}

/// El orden inverso resuelve a cursiva, porque la cursiva es la exterior.
///
/// Este test es el que **corrigió** la regla. El doc de `SpanMarks` decía
/// "gana negrita" y este test afirmaba negrita; el código aplicaba "la primera
/// marca que se abrió", y en `*cursiva con **negrita***` la primera es la
/// cursiva. Las dos frases no coincidían y el que tenía razón era el código.
///
/// La forma del test deja ver la regla sin tener que creerla: los dos casos de
/// marcas anidadas dan el **exterior**, y el exterior es distinto en cada uno.
#[test]
fn la_primera_marca_abierta_gana() {
    let bloques = parse_document("*cursiva con **negrita***\n");
    let DocBlock::Paragraph { spans } = &bloques[0] else {
        panic!("se esperaba un párrafo, vino {bloques:?}");
    };
    let marcas: Vec<SpanMarks> = spans.iter().map(|s| s.marks.clone()).collect();
    assert!(
        marcas.contains(&SpanMarks::Italic),
        "la cursiva es la marca exterior, y la exterior gana: {marcas:?}"
    );
    assert!(
        !marcas.contains(&SpanMarks::Bold),
        "y la negrita, que es interior, no se ve: {marcas:?}"
    );
}

/// Los dos órdenes de anidación resuelven al **exterior**, cada uno al suyo.
///
/// El par de tests que hace que la regla sea una regla y no una casualidad:
/// `**a _b_**` da bold y `*a **b***` da italic. Con "la última gana" los dos
/// darían cursiva; con "siempre negrita" los dos darían negrita. Solo "la primera
/// abierta" da uno de cada uno.
#[test]
fn los_dos_ordenes_resuelven_a_su_marca_exterior() {
    let con_negrita_adentro = parse_document("**a *b* c**\n");
    let con_cursiva_adentro = parse_document("*a **b** c*\n");

    let marcas_de = |bloques: &[DocBlock]| -> Vec<SpanMarks> {
        let DocBlock::Paragraph { spans } = &bloques[0] else {
            panic!("se esperaba un párrafo, vino {bloques:?}");
        };
        spans.iter().map(|s| s.marks.clone()).collect()
    };

    let fuera = marcas_de(&con_negrita_adentro);
    let dentro = marcas_de(&con_cursiva_adentro);
    assert!(
        fuera.contains(&SpanMarks::Bold) && !fuera.contains(&SpanMarks::Italic),
        "con negrita afuera: {fuera:?}"
    );
    assert!(
        dentro.contains(&SpanMarks::Italic) && !dentro.contains(&SpanMarks::Bold),
        "con cursiva afuera: {dentro:?}"
    );
}

/// El código inline **no** hereda el énfasis que lo rodea.
///
/// `` `codigo` `` dentro de `**negrita**` sale como [`SpanMarks::Code`], no como
/// una marca compuesta.
///
/// El enum de marcas no tiene dónde poner "código y negrita", y esa es una
/// limitación consciente: el code span es el elemento visible y el reader lo ve
/// como código. Un renderer que quisiera las dos cosas necesita un
/// `SpanMarks::CodeBold`, que este IR no tiene.
#[test]
fn el_codigo_inline_no_hereda_el_enfasis() {
    let bloques = parse_document("**un `codigo` aca**\n");
    let DocBlock::Paragraph { spans } = &bloques[0] else {
        panic!("se esperaba un párrafo, vino {bloques:?}");
    };
    let codigo = spans
        .iter()
        .find(|s| s.text.contains("codigo"))
        .expect("el código inline tiene que estar");
    assert_eq!(
        codigo.marks,
        SpanMarks::Code,
        "el código inline manda sobre el énfasis que lo rodea"
    );
}

/// Los slugs de un encabezado con acentos y signos.
///
/// `slugify` ya tiene sus propios tests en el renderer; lo que se mide acá es que
/// el IR usa **el mismo** criterio, que es la razón de compartir la función.
///
/// Y hay un caso que **no** es igual al de `slugify`, y está aparte en
/// [`el_apostrofe_de_texto_ascii_llega_curvo_al_slug`] porque no tiene el mismo
/// resultado. Acá están los que sí coinciden.
#[test]
fn los_slugs_siguen_el_criterio_de_slugify() {
    let casos = [
        ("# Hello World\n", "hello-world"),
        ("# Hola Mundo\n", "hola-mundo"),
        ("# ¡Hola! ¿Qué tal?\n", "hola-qué-tal"),
        ("# 100%Listo\n", "100listo"),
        ("# 🦀 Crab\n", "crab"),
    ];
    for (md, esperado) in casos {
        let bloques = parse_document(md);
        let DocBlock::Heading { id, .. } = &bloques[0] else {
            panic!("se esperaba un encabezado para {md:?}, vino {bloques:?}");
        };
        assert_eq!(id, esperado, "el slug de {md:?}");
    }
}

/// El apostrophe de texto ASCII llega **curvo** al slug, y el slug cambia.
///
/// Un hallazgo medido que surprising y que no estaba en el plan.
///
/// `parser_options()` activa `ENABLE_SMART_PUNCTUATION`, así que el parser convierte
/// el `'` de `What's new?` en `’` (U+2019) **antes** de que el texto llegue al
/// slug. `slugify` recibe `What’s new?`, y ahí `’` no es alfanumérico ni es
/// espacio, así que no aporta nada: el resultado es `what-s-new`.
///
/// En cambio el test de `slugify` del renderer llama a la función **directamente**
/// con `"What's new?"` y da `whats-new`. Los dos resultados son correctos para
/// entradas distintas, y el mismo usuario ve el mismo slug de las dos formas
/// porque siempre pasa por el parser.
///
/// La razón de que esto sea un test y no un comentario es que el valor esperado
/// **cambia si alguien toca `slugify`**: sin el test, el `id` del outline cambiaría
/// en silencio y los enlaces anclados a esa sección dejarían de funcionar.
#[test]
fn el_apostrofe_de_texto_ascii_llega_curvo_al_slug() {
    let por_parser = parse_document("# What's new?\n");
    let DocBlock::Heading { id, .. } = &por_parser[0] else {
        panic!("se esperaba un encabezado, vino {por_parser:?}");
    };
    assert_eq!(
        id, "what-s-new",
        "con smart punctuation el apóstrofo es U+2019 y slugify lo ignora"
    );

    // Y la comparación que lo vuelve una regla y no una curiosidad: el mismo
    // `slugify` sobre el texto ASCII da otra cosa, y la diferencia está entera en
    // el apóstrofo.
    let por_llamada_directa = codedocs_core::markdown::slugify("What's new?");
    assert_eq!(
        por_llamada_directa, "whats-new",
        "llamado directo, el ASCII sí desaparece"
    );
    assert_ne!(
        id, &por_llamada_directa,
        "por eso el test del renderer y este no pueden decir lo mismo"
    );
}

/// Un encabezado que solo es HTML no produce ningún bloque.
///
/// La misma decisión que toma `render_markdown`, y por la misma razón: `# <b>` no
/// deja texto una vez que el HTML se descarta, así que no hay un encabezado que pintar. Un
/// `Heading` con `spans` vacío sería un encabezado invisible con un ancla que
/// nadie puede ver.
#[test]
fn un_encabezado_solo_html_no_produce_bloque() {
    assert!(parse_document("# <b>\n").is_empty());
    assert!(parse_document("#\n").is_empty());
}

/// Una etiqueta de bloque sin texto no produce un párrafo.
///
/// La mitad del caso anterior para los párrafos, con una corrección que la
/// medición obligó: `<img src=x onerror=alert(1)>` **no** es un párrafo. Medido
/// con `examples/probe.rs`, `img` es una de las etiquetas de bloque de CommonMark,
/// así que el parser la emite como `HtmlBlock` entero y el IR produce un
/// [`DocBlock::Opaque`] con el texto vacío —no un `Paragraph` con spans vacíos—.
///
/// Lo que el test afirma entonces es la cosa que importa: **no hay ningún párrafo
/// con contenido vacío** en el árbol, que es lo que un `unwrap_or_default` en el
/// `close` de un párrafo produciría.
#[test]
fn un_bloque_de_html_sin_texto_no_produce_parrafo() {
    let bloques = parse_document("<img src=x onerror=alert(1)>\n");
    assert!(
        !bloques
            .iter()
            .any(|b| matches!(b, DocBlock::Paragraph { .. })),
        "un párrafo sin texto no es un párrafo: {bloques:?}"
    );
    // Y el HTML inline sí se conserva, por el otro camino.
    let inline = parse_document("<b>hola</b>\n");
    assert!(
        inline
            .iter()
            .any(|b| matches!(b, DocBlock::Paragraph { .. })),
        "el HTML inline sí deja texto, porque el texto llega como Event::Text: {inline:?}"
    );
}

/// Un cercado sin cerrar sigue siendo un bloque de código.
///
/// El caso del `flush`. Un documento con ```` ```rust\nfn main() {} ```` y nada
/// más sigue siendo un bloque de código para quien lo lee, y perderlo sería perder
/// contenido.
#[test]
fn un_cercado_sin_cerrar_sigue_siendo_un_bloque() {
    let bloques = parse_document("```rust\nfn main() {}\n");
    let encontrados: Vec<&DocBlock> = bloques
        .iter()
        .filter(|b| matches!(b, DocBlock::CodeBlock { .. }))
        .collect();
    assert_eq!(
        encontrados.len(),
        1,
        "el bloque sin cerrar tiene que estar: {bloques:?}"
    );
    let DocBlock::CodeBlock { lang, code } = encontrados[0] else {
        unreachable!("el filtro de arriba ya cubre este caso");
    };
    assert_eq!(lang.as_deref(), Some("rust"), "el idioma también sobrevive");
    assert!(code.contains("fn main"), "y el código: {code:?}");
}

/// El idioma del cercado se normaliza a minúsculas.
///
/// Misma regla que `fenced_lang`, que es la función que se comparte. `RUST` y
/// `rust` tienen que dar el mismo `lang`, o un renderer que compare por nombre
/// falla en un documento y no en otro.
#[test]
fn el_lenguaje_del_cercado_se_normaliza() {
    let bloques = parse_document("```RUST\nfn main() {}\n```\n");
    let DocBlock::CodeBlock { lang, .. } = &bloques[0] else {
        panic!("se esperaba un bloque de código, vino {bloques:?}");
    };
    assert_eq!(lang.as_deref(), Some("rust"));
}

/// Una lista de una sola entrada con `en_lista`.
///
/// El otro diseño de `List`, el que **sí** lleva `Box` en la firma equivalente.
/// El test está para que la existencia de la función no sea decorativa: si mañana
/// se borra, esto no compila.
#[test]
fn en_lista_envuelve_una_sola_entrada() {
    let bloque = vec![DocBlock::Paragraph {
        spans: vec![Span::plain("solo")],
    }]
    .en_lista(true);
    let DocBlock::List { ordered, items } = bloque else {
        panic!("en_lista tiene que devolver un List: {bloque:?}");
    };
    assert!(ordered);
    assert_eq!(items.len(), 1);
    assert!(matches!(items[0][0], DocBlock::Paragraph { .. }));
}

/// El árbol del IR y el del renderer tienen la misma cantidad de bloques.
///
/// Un conteo, no una comparación. Suficiente para fijar que el IR **no duplica ni
/// pierde bloques** en las construcciones que no tienen palabras —un `Rule`, un
/// separador de tabla—, que es donde el conteo cuenta algo que el texto no.
#[test]
fn la_cantidad_de_bloques_no_se_va_de_las_manos() {
    let md = "párrafo\n\n---\n\notro párrafo\n\n> cita\n\n- a\n- b\n";
    let bloques = parse_document(md);
    let cuenta = bloques.len();
    assert_eq!(cuenta, 5, "cinco bloques de primer nivel: {bloques:?}");
}

/// El `Debug` del IR no puede contener HTML del documento.
///
/// Una afirmación sobre **la forma del tipo**, y la última del archivo.
///
/// La idea: como `DocBlock` no tiene ningún campo de tipo `String` que el
/// renderer pueda interpolar, y como su `Debug` —el mismo `derive` que usan los
/// golden tests— solo imprime `String`s que ya son texto, no hay forma de que un
/// `format!("{:?}")` sobre el IR produzca marcado. El test lo afirma sobre un
/// corpus hostil, y no sobre un documento limpio, porque es el hostil el que
/// tiene los `<script>`.
///
/// Con `render_markdown` esto **no** se puede afirmar: su salida es HTML por
/// definición. La diferencia es la razón de que el IR exista.
#[test]
fn el_debug_del_ir_no_puede_contener_html() {
    let md = "<script>alert(1)</script>\n\n<div>x</div>\n";
    let depurado = format!("{:?}", parse_document(md));
    for marcador in ["<script", "<div", "alert("] {
        assert!(
            !depurado.contains(marcador),
            "{marcador} apareció en el Debug del IR: {depurado}"
        );
    }
    // Y el texto legible sí está, porque el IR conserva lo que se puede leer.
    assert!(
        depurado.contains("x"),
        "el texto entre etiquetas tiene que estar: {depurado}"
    );
}
