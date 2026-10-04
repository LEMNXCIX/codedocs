# Editor en formato vivo — plan de implementación

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Que el editor muestre el markdown formateado mientras se escribe, sin dejar de editar markdown plano.

**Architecture:** `codedocs-core` calcula "tramos" (rangos + etiqueta) con `pulldown-cmark` en modo offset, y los cruza a JavaScript como dos `TypedArray`. Un `ViewPlugin` de CodeMirror 6 los convierte en `Decoration`s. El documento de CodeMirror sigue siendo el markdown: no hay serializador DOM→markdown, así que el archivo no puede corromperse.

**Tech Stack:** Rust (`pulldown-cmark` 0.13), Leptos 0.8 + wasm-bindgen, CodeMirror 6, Tailwind, esbuild, Playwright.

**Spec:** `docs/superpowers/specs/2026-10-03-live-format-editor-design.md`

## Global Constraints

- El markdown es el documento. Nada de serializar el DOM a markdown.
- Todo tramo que se cruce a JS va en **unidades UTF-16**, nunca en bytes.
- El documento que se guarda tiene que ser exactamente lo que se escribió. Cualquier prueba que escriba en el editor debe comparar con `__codedocs_getContent()`.
- `script-src 'self' 'wasm-unsafe-eval'` no se relaja. Nada de `innerHTML` con contenido del documento.
- Los cargadores de KaTeX y Mermaid siguen siendo perezosos y cacheados por promesa.
- Todo comando nuevo declarado en Rust tiene que existir en la tabla de `SHORTCUTS` o en `shortcut_for`, o los tests `every_command_has_a_shortcut` y `shortcuts_are_unique` fallan.
- El usuario escribe en español rioplatense. Los mensajes de error y de UI van en español.

## Review Focus

Cinco entradas que el spec implica pero que ninguna tarea cubre con un test propio. Cada una tiene su test en la tarea indicada.

1. **Marcadores desbalanceados.** `**negrita` sin cerrar, `[texto](` a medio escribir, `|` sueltos. No deben tragarse texto ni dejar el cursor perdido. → Task 1 (casos degenerados) y Task 3 (browser).
2. **Composición IME.** Escribir acentos con teclado virtual o un emoji. La composición no debe romper las decoraciones ni dejar el cursor donde no era. → Task 3.
3. **Deshacer y rehacer.** `Ctrl+Z` tiene que seguir siendo atómico y las decoraciones tienen que reaparecer cuando los `**` vuelven al documento. → Task 3.
4. **Enter dentro de un bloque de código.** Tiene que insertar un salto de línea literal, no partir el bloque ni continuar una lista. → Task 5.
5. **Pegar markdown desde el portapapeles** dentro de una línea ya formateada. Las decoraciones tienen que recalcularse y el contenido guardado tiene que coincidir con lo pegado. → Task 3.

---

## Mapa de archivos

| Archivo | Responsabilidad |
|---|---|
| `codedocs-core/src/markdown/live.rs` | Calcula los tramos. Índice de líneas para UTF-16, regla del marcador, resolución de solapamientos. |
| `codedocs-core/src/markdown/mod.rs` | Expone `live_spans`; comparte `parser_options()` (bajar a `pub(crate)`). |
| `codedocs-core/src/lib.rs` | Re-exporta `live_spans`, `LiveSpans`, `SpanTag`. |
| `codedocs-core/tests/live_invariants.rs` | Invariantes sobre documentos arbitrarios. |
| `codedocs-core/examples/live.rs` | Vuelca tramos de un archivo real, para que el script de navegador use datos del parser real. |
| `js/live-preview.mjs` | `ViewPlugin`: construye decoraciones, aplica la regla de revelado, define los widgets. |
| `js/markdown-input.mjs` | Keymap de escritura: Enter en listas, Tab/Shift-Tab, reglas de tipeo. |
| `js/codemirror-bridge.mjs` | Arma el `EditorState` con las extensiones nuevas; expone `__codedocs_setSpanProvider` y `__codedocs_setSourceVisible`. |
| `js/preview-bridge.mjs` | Deja de escanear `document`; expone `renderMathInto(el, source, display)` y `renderMermaidInto(el, source)`. |
| `src/state.rs` | `source_mode` reemplaza a `view_mode`. |
| `src/components/editor/mod.rs` | `EditorPane` sin `Preview` ni `preview_html`. |
| `src/components/editor/codemirror.rs` | Registra el proveedor de tramos, empuja `source_mode`, tabla de comandos. |
| `src/components/layout.rs` | Saca `preview_html` y el render por tecla. |
| `src/shortcuts.rs` | `ToggleSource` en lugar de los cuatro comandos de modo. |
| `input.css` | Clases `.cm-lp-*` con variantes `dark:`. |
| `scripts/check-live-editor.cjs` | Verificación del editor en Chromium. |
| `scripts/check-bridge-contract.mjs` | Agrega `js/live-preview.mjs` y `js/markdown-input.mjs` a `JS_FILES`. |
| `scripts/check-preview-render.cjs` | Pasa a verificar los widgets del editor. |

---

### Task 0: Spike del cursor con texto oculto (COMPLETO)

**Resultado: gana `Decoration.mark` + `font-size: 0`.** Informe con las mediciones:
`.superpowers/sdd/2026-10-03-live-format-editor/spike/RESULT.md`.

Lo que decide:

| | técnica | resultado |
|---|---|---|
| usar | `Decoration.mark({ class: "cm-lp-hidden" })` + `.cm-lp-hidden { font-size: 0 }` | oculta, caret de a uno, copiar conserva el markdown |
| descartar | `Decoration.replace({})` | el caret se traba y **copiar pierde el `**` de apertura** |
| descartar | `Decoration.mark` + `display: none` | el caret queda atascado en el offset 2 para siempre |

Los pasos que siguen están como referencia de cómo se midió, no como trabajo
pendiente.

- [ ]x **Step 1: Montar el spike**

Con esbuild, empaquetar un `main.mjs` que cree un `EditorView` de CodeMirror 6 con `doc = "**hola**"` y cuatro extensiones de prueba:

| # | Técnica |
|---|---|
| a | `Decoration.replace({})` sobre el rango `0..2` |
| b | `Decoration.replace({ widget: new WidgetType(() => null) })` |
| c | `Decoration.mark({ class: "spike-hidden" })` con `.spike-hidden { display: none; }` |
| d | `Decoration.mark({ class: "spike-hidden" })` con `.spike-hidden { font-size: 0; }` |

Salida `index.html` y `bundle.js` en `/tmp/opencode/spike-caret/`, servidos por el mismo servidor http que usan los otros scripts del repo.

- [ ]x **Step 2: Medir el comportamiento del caret**

Script de Playwright que, para cada técnica, ejecute y registre:

| Caso | Qué se mide |
|---|---|
| Caret al final del documento, 8 flechas a la izquierda | Offset del caret tras cada una; debería bajar de a uno, no de a tres |
| Escribir `x` con el caret al final | `view.state.doc.toString()` debe ser `**hola**x`: el caret está en el offset 8 y el carácter entra ahí |
| Click entre las dos estrellas | Offset resultante |
| Seleccionar todo | `textContent` de la línea |
| Consola | Ninguna excepción en `pageerror` |

- [ ]x **Step 3: Decidir y reportar**

Anotar en `/tmp/opencode/spike-caret/RESULT.md`: técnica elegida y por qué. Gana la primera que no tire excepción y que pase los cuatro casos.

- [ ]x **Step 4: Eliminar el spike**

`rm -rf /tmp/opencode/spike-caret`. Nada de esto entra al repo.

**Gate:** si ninguna técnica pasa los cuatro casos, **parar y volver al usuario**. El plan B del spec (revelar siempre la línea del cursor) cambia el diseño y necesita aprobación.

---

### Task 1: `live_spans` en `codedocs-core`

**Files:**
- Create: `codedocs-core/src/markdown/live.rs`
- Modify: `codedocs-core/src/markdown/mod.rs`, `codedocs-core/src/lib.rs`
- Test: `codedocs-core/tests/live_invariants.rs`

**Interfaces:**
- Consumes: `markdown::parser_options()` (bajar a `pub(crate)`).
- Produces:
  ```rust
  pub enum SpanTag { Hide = 0, Strong = 1, Emphasis = 2, Strikethrough = 3,
      InlineCode = 4, LinkText = 5, ImageAlt = 6, Heading1 = 7, /* .. Heading6 = 12 */
      Quote = 13, ListMarker = 14, Fence = 15, TablePipe = 16,
      TableDelimiter = 17, MathInline = 18, MathDisplay = 19, MermaidBlock = 20 }

  pub struct LiveSpans {
      pub ranges: Vec<u32>,   // [from0, to0, from1, to1, ...] en UTF-16
      pub tags: Vec<SpanTag>,
  }

  pub fn live_spans(content: &str) -> LiveSpans
  ```
  `LiveSpans` implementa `Default`, `Debug`, `Clone`, `PartialEq`, `Eq`.

- [ ] **Step 1: Escribir los tests que fallan**

En `live.rs`, `mod tests`, con un helper `fn spans(src: &str) -> Vec<(u32, u32, SpanTag)>` que aplana el `LiveSpans`:

```rust
#[test] fn heading_marker_is_hidden() {
    // "# Hola" -> un Hide en 0..2 y un Heading1 en 2..6
}

#[test] fn strong_hides_both_delimiters() {
    // "**a**" -> Hide 0..1, Strong 1..2, Hide 2..3
}

#[test] fn offsets_are_utf16_not_bytes() {
    // "# ñ😀": el tramo oculto mide 0..2 en UTF-16, aunque en bytes sea 0..3.
    // Con bytes, el texto marcado empezaría en la posición 3 y sería "ñ😀" en vez
    // de "ñ😀" con el emoji partido. Este es el test que más importa.
}

#[test] fn link_hides_bracket_and_url() {
    // "[t](u)" -> Hide 0..1 (el "["), LinkText 1..2, Hide 2..5 ("](u)")
}

#[test] fn degenerate_input_does_not_panic() {
    // "**", "[]()", "####### x", "#", "~~~", "|---|", "$", "$$", "- [ ]"
    // Ninguno entra en panic y ninguno devuelve un tramo con from > to.
}

#[test] fn nested_emphasis_has_no_overlapping_hides() {
    // "**a *b* c**": ningún par de Hide se solapa.
}
```

En `codedocs-core/tests/live_invariants.rs` (integración, sobre documentos arbitrarios):

```rust
#[test] fn spans_are_sorted_and_within_bounds()
#[test] fn no_span_exceeds_the_utf16_length_of_the_document()
#[test] fn empty_document_produces_no_spans()
#[test] fn spans_are_stable_under_reparse()   // live_spans(x) == live_spans(x)
```

- [ ] **Step 2: Correr los tests y ver que fallan**

Run: `cargo test -p codedocs-core live`
Expected: error de compilación — `live_spans` no existe todavía.

- [ ] **Step 3: Implementar el índice UTF-16**

En `live.rs`, tipo privado (los tests de UTF-16 van en el mismo módulo, que es como `tree.rs` hace con los suyos):

```rust
/// Inicios de línea en índices de byte y en unidades UTF-16, para traducir los
/// offsets de `pulldown-cmark` a los que cuenta CodeMirror.
struct Utf16Index { /* ... */ }

impl Utf16Index {
    fn new(content: &str) -> Self
    fn to_utf16(&self, byte_offset: usize) -> Option<u32>
}
```

`to_utf16` busca la línea por búsqueda binaria y suma `chars().map(char::len_utf16)` sobre lo que precede al offset dentro de la línea. Devuelve `None` si el offset está fuera del documento.

- [ ] **Step 4: Implementar la regla del marcador**

Recorrer `Parser::new_ext(content, parser_options()).into_offset_iter()`. Para cada `Event::Start(tag)` con rango `(start, end)`, el marcador es **la parte de `(start, end)` no cubierta por sus hijos**, y se emite un `Hide` por tramo. El marcador inicial es `start..primer_hijo_start` y el final `último_hijo_end..end`; ambos se emiten sólo si hay un hijo al que pegarse, porque una construcción sin hijos no tiene dónde poner el marcador.

Según el `Tag`, emitir además el `SpanTag` de estilo:

| `Tag` | Rango del tramo de estilo | Etiqueta |
|---|---|---|
| `Heading { level }` | `start..end` | `Heading{level}` |
| `Strong` | hijo | `Strong` |
| `Emphasis` | hijo | `Emphasis` |
| `Strikethrough` | hijo | `Strikethrough` |
| `Code` (inline) | hijo | `InlineCode` |
| `Link` | hijo | `LinkText` |
| `Image` | hijo | `ImageAlt` |
| `BlockQuote` | `start..primer_hijo` | `Quote` |
| `Item` | del inicio del ítem al primer hijo | `ListMarker` |
| `CodeBlock(Fenced(lang))` | vallas de apertura y cierre | `Fence` |
| `Table` y celdas | cada `\|` y la fila `\|---\|` | `TablePipe`, `TableDelimiter` |

`MathInline`, `MathDisplay` y `MermaidBlock` los emiten `markdown/math.rs` (que ya existe) y la detección de un `CodeBlock` cuyo `info` sea `mermaid`.

- [ ] **Step 5: Convertir y resolver**

Pasar cada rango de byte por `Utf16Index::to_utf16`. Ordenar por `from`. Al agregar un `Hide`, descartar los `Hide` ya presentes que se solapen con él: gana el externo, según el spec.

- [ ] **Step 6: Correr los tests y ver que pasan**

Run: `cargo test -p codedocs-core`
Expected: todos pasan, incluidos los 81 preexistentes.

- [ ] **Step 7: Lint y formato**

Run: `cargo fmt --all && cargo clippy -p codedocs-core --all-targets -- -D warnings`
Expected: sin salida.

- [ ] **Step 8: Commitear**

```bash
git add codedocs-core/
git commit -m "feat(core): live_spans emite tramos de markdown en UTF-16"
```

---

### Task 2: El ejemplo `live.rs` y el puente a wasm

**Files:**
- Create: `codedocs-core/examples/live.rs`
- Modify: `src/components/editor/codemirror.rs`

**Interfaces:**
- Consumes: `codedocs_core::live_spans(&str) -> LiveSpans` (Task 1).
- Produces, en Rust (frontend):
  ```rust
  #[wasm_bindgen(js_name = __codedocs_setSpanProvider)]
  fn set_span_provider(callback: &js_sys::Function);

  #[wasm_bindgen(js_name = __codedocs_setSourceVisible)]
  fn set_source_visible(is_visible: bool);
  ```
  El callback recibe un `String` y devuelve un `js_sys::Array` de dos elementos: un `js_sys::Uint32Array` con `ranges` y un `js_sys::Uint8Array` con los discriminantes de `tags`.

- [ ] **Step 1: Escribir el ejemplo**

`codedocs-core/examples/live.rs`: lee la ruta del primer argumento, llama a `live_spans` e imprime JSON con `ranges` y `tags` (el `#[repr(u8)]` como número). A mano, como `examples/walk.rs`, para no agregar dependencia de serialización.

- [ ] **Step 2: Verificar el ejemplo con acentos y emoji**

Run: `printf '# \303\261\360\237\230\200 **b**' > /tmp/opencode/t.md && cargo run -q -p codedocs-core --example live -- /tmp/opencode/t.md`
Expected: `ranges` arranca con `0,2` (el `# ` en UTF-16), no `0,3`.

- [ ] **Step 3: Declarar las dos funciones en `codemirror.rs`**

Agregar al `extern "C"`. Los `js_name` tienen que coincidir exactamente con lo que exporte `js/codemirror-bridge.mjs` en el Task 3; ese es el contrato que `check-bridge-contract.mjs` verifica.

- [ ] **Step 4: Registrar el proveedor**

En `CodeMirrorEditor`, siguiendo el patrón de `cm_set_on_change`: crear un `Closure` que llame a `codedocs_core::live_spans`, arme los dos `TypedArray` y devuelva el `Array`. Pasarlo a `set_span_provider` y hacer `forget()` — el `Closure` tiene que sobrevivir a la función, igual que los otros dos.

Usar `Closure::<dyn FnMut(String) -> JsValue>::new(...)` con `as_ref().unchecked_ref()`. No `dyn Fn(String) -> JsValue`: la firma de `Closure` es `FnMut(A) -> R` con `A: Into<JsValue>` y `R: Into<JsValue>`.

- [ ] **Step 5: Verificar que compila**

Run: `cargo check --target wasm32-unknown-unknown`
Expected: sin errores.

- [ ] **Step 6: Commitear**

```bash
git add codedocs-core/examples/live.rs src/components/editor/codemirror.rs
git commit -m "feat: puente de tramos entre wasm y el editor"
```

---

### Task 3: `js/live-preview.mjs` — ocultar, estilizar y revelar

**Files:**
- Create: `js/live-preview.mjs`, `scripts/check-live-editor.cjs`
- Modify: `js/codemirror-bridge.mjs`, `input.css`, `package.json`, `Trunk.toml`, `index.html`, `scripts/check-bridge-contract.mjs`

**Interfaces:**
- Consumes: `__codedocs_setSpanProvider(fn)`, `__codedocs_setSourceVisible(bool)` (Task 2); la técnica que ganó el spike del Task 0.
- Produces:
  ```js
  // js/live-preview.mjs
  export function livePreviewExtension(options)   // -> Extension de CodeMirror
  window.__codedocs_setSpanProvider = function (callback) { ... }
  window.__codedocs_setSourceVisible = function (isVisible) { ... }
  ```
  `options`: `{ isSourceVisible: () => boolean }`.

- [ ] **Step 1: Escribir el script de verificación que falla**

`scripts/check-live-editor.cjs`, con la misma estructura de servidor + stub que `scripts/check-ipc-flow.cjs`: CSP con los hashes de los scripts inline, y `tauri-stub.js` antes del bundle. Casos:

| Caso | Aserción |
|---|---|
| Escribir `**hola**` | `__codedocs_getContent()` es exactamente `**hola**` |
| Seleccionar todo y copiar | el portapapeles conserva `**hola**`, no `hola**` |
| Lo anterior | existe un `.cm-lp-strong` en el DOM |
| Lo anterior | el `**` no se ve: la línea no tiene rects de cliente extras |
| Cursor dentro de la línea | el texto visible vuelve a ser `**hola**` |
| `Ctrl+/` | no queda ninguna decoración de ocultado en el DOM |
| Deshacer hasta borrar los `**` y rehacer | el contenido vuelve a `**hola**` y `.cm-lp-strong` reaparece |
| Composición IME | simular con `Input.insertText`; el caret queda donde se insertó |
| Pegar `**x**` con el portapapeles | el contenido es el pegado exacto |
| Marcadores desbalanceados | `**negrita` sin cerrar no traga texto ni rompe el editor |
| Documento de 5 000 líneas | mediana de 20 pulsaciones, del `dispatch` al `requestAnimationFrame` siguiente, **< 50 ms** |

Para el texto visible de una línea, usar `Range.selectNodeContents(lineElement)` y `getClientRects().length`: los nodos ocultos con `display: none` no tienen rects.

- [ ] **Step 2: Correrlo y ver que falla**

Run: `npm run build && node scripts/check-live-editor.cjs`
Expected: FAIL — `__codedocs_setSpanProvider` todavía no existe en el bundle.

- [ ] **Step 3: Agregar el bundle al pipeline**

En `package.json`:

```json
"build:live": "esbuild js/live-preview.mjs --bundle --format=iife --target=es2020 --outfile=public/live-preview.js --minify"
```

En `Trunk.toml`, un `[[hooks]]` de `pre_build` equivalente al de `preview-bridge`, y `"./public/live-preview.js"` en `watch.ignore`.

En `index.html`, `<script src="/public/live-preview.js"></script>` **después** de `codemirror-bridge.js`.

En `scripts/check-bridge-contract.mjs`, agregar `"js/live-preview.mjs"` a `JS_FILES`. Sin esto el script no verifica los globals nuevos y el contrato vuelve a quedar a medias, que es exactamente el modo de fallo que ya costó un crash.

- [ ] **Step 4: Implementar la construcción de decoraciones**

En `js/live-preview.mjs`:

```js
import { Decoration, ViewPlugin } from "@codemirror/view";
import { RangeSetBuilder } from "@codemirror/state";
```

`buildDecorations(view)`:

1. `const { ranges, tags } = spanProvider(view.state.doc.toString())`.
2. Calcular el conjunto de líneas reveladas: para cada rango de `view.state.selection.ranges`, `view.state.doc.lineAt(r.from).number` y `lineAt(r.to).number`.
3. `const showAll = isSourceVisible()`.
4. Crear el builder con el terminador primero: `builder.add(doc.length, Decoration.none)`. Es el idioma de `RangeSetBuilder` y evita que el primer tramo tenga que empezar en 0.
5. Llenar el builder con los tramos en orden de `from`, saltando los de una línea revelada si `!showAll`.

`decorationFor(tag)` mapea `SpanTag` a decoraciones. La fila `Hide` usa lo que ganó el spike del Task 0, ya medido:

```js
// Decoration.mark y Decoration.replace son metodos estaticos que devuelven una
// Decoration: NO llevan `new`. `DecorationSet` no se exporta como valor, asi que
// el conjunto se arma con RangeSetBuilder, que es una clase.
import { RangeSetBuilder } from "@codemirror/state";
import { Decoration } from "@codemirror/view";

const builder = new RangeSetBuilder();
for (const { from, to, value } of ranges) builder.add(from, to, value);
const set = builder.finish();
```


| Tag | Decoración |
|---|---|
| `Hide` (0) | `Decoration.mark({ class: "cm-lp-hidden" })`, oculto con `font-size: 0` |
| `Strong`, `Emphasis`, `Strikethrough`, `InlineCode` (1-4) | `Decoration.mark({ class: "cm-lp-strong" })`, etc. |
| `LinkText` (5) | `Decoration.mark({ class: "cm-lp-link" })` |
| `ImageAlt` (6) | widget — Task 4 |
| `Heading1..6` (7-12) | `Decoration.line({ class: "cm-lp-h1" })`, etc. |
| `Quote` (13) | `Decoration.line({ class: "cm-lp-quote" })` |
| `ListMarker` (14) | ocultar, como `Hide` |
| `Fence` (15) | ocultar, como `Hide` |
| `TablePipe` (16), `TableDelimiter` (17) | ocultar, como `Hide` |

**No usar `Decoration.replace` para ocultar.** El spike lo midió: el caret se
traba y, peor, seleccionar todo y copiar devuelve `"hola**x"` en vez de
`"**hola**x"` — el markdown de apertura desaparece y el usuario no se entera
hasta que abre el archivo en otro programa. **No usar `display: none`**: el
caret queda atascado en el offset 2 para siempre.
| `MathInline` (18), `MathDisplay` (19), `MermaidBlock` (20) | widget — Task 4 |

**Dos clases de decoración, dos reglas de rango.** `Decoration.line` cubre una línea
entera (`line.from → line.to`), no un rango suelto, y su rango no puede quedar
dentro de un reemplazo de bloque. Por eso el builder no se llena con un único
recorrido:

1. **Recorrido de rango** para `Hide`, `mark` y los widgets inline, en orden de `from`.
2. **Recorrido de línea** para los `line` decorations, una vez por línea y en orden de línea.

Y hay una regla de prioridad que no es obvia: cuando un widget de bloque (math
display, mermaid) cubre una línea, esa línea **no** recibe `line` decoration. Un
rango de reemplazo de bloque y un rango de línea sobre las mismas posiciones
hacen que `RangeSetBuilder` rechace el conjunto entero, y el síntoma es que
*ninguna* decoración de ese documento aparece: un fallo que se lee como "el motor
no anda" y no como "dos decoraciones chocaron".

Las decoraciones se cachean en el `ViewPlugin` y sólo se recalculan si cambia el documento, la selección o `sourceVisible`.

- [ ] **Step 5: Cablear en `codemirror-bridge.mjs`**

- `getExtensions` suma `livePreviewExtension({ isSourceVisible: () => sourceVisible })`.
- `window.__codedocs_setSpanProvider` guarda el callback en una variable de módulo.
- `window.__codedocs_setSourceVisible` guarda el booleano y fuerza el recálculo con un `StateEffect` que el `ViewPlugin` observa.
- `window.__codedocs_destroyEditor` limpia también el proveedor y el booleano.

- [ ] **Step 6: El CSS**

En `input.css`, bloque para `.cm-editor`:

```css
/* Ocultado de sintaxis. font-size: 0 y NO display: none: con display: none el
   caret queda atascado en el offset 2 y no puede atravesar el marcador. */
.cm-lp-hidden { font-size: 0; }

.cm-lp-strong { font-weight: 700; }
.cm-lp-emphasis { font-style: italic; }
.cm-lp-strikethrough { text-decoration: line-through; }
.cm-lp-inlinecode { background: <color>; padding: 0 0.3em; border-radius: 0.25rem; }
.cm-lp-link { color: <color>; text-decoration: underline; }
.cm-lp-h1 { font-size: 1.6rem; font-weight: 700; line-height: 1.3; }
.cm-lp-h2 { font-size: 1.35rem; font-weight: 700; }
/* h3 a h6 */
.cm-lp-quote { border-left: 3px solid <color>; padding-left: 0.75rem; font-style: italic; }
```

Cada uno con su variante `dark:`. Sin esto la vista en vivo queda ilegible de noche, que es el riesgo 4 del spec.

Los estilos de `code`, `table`, `blockquote` y `h1..h6` que ya existen vía `@tailwindcss/typography` **no aplican** dentro de CodeMirror: el `.prose` está en el div de la preview, que este task borra. Hay que escribirlos.

- [ ] **Step 7: Correr el script y ver que pasa**

Run: `npm run build && node scripts/check-live-editor.cjs`
Expected: PASS en los casos de la tabla.

- [ ] **Step 8: Verificar el contrato**

Run: `npm run check:bridge`
Expected: PASS, contando ya los globals nuevos.

- [ ] **Step 9: Commitear**

```bash
git add js/ input.css package.json Trunk.toml index.html scripts/ public/live-preview.js
git commit -m "feat: vista en vivo en el editor con revelado por línea del cursor"
```

---

### Task 4: Widgets de math, Mermaid, tabla e imagen

**Files:**
- Modify: `js/live-preview.mjs`, `js/preview-bridge.mjs`, `scripts/check-live-editor.cjs`

**Interfaces:**
- Consumes: de `js/preview-bridge.mjs`, `window.__codedocs_renderMathInto(el, source, isDisplay)` y `window.__codedocs_renderMermaidInto(el, source)`. Ambas devuelven una `Promise` y **no lanzan**: si KaTeX o Mermaid no cargan, dibujan el código fuente con construcción DOM, nunca con `innerHTML`.
- Produces: los tags 6, 18, 19 y 20 pasan de "widget" a widget implementado.

- [ ] **Step 1: Escribir los tests que fallan**

Agregar a `scripts/check-live-editor.cjs`:

| Caso | Aserción |
|---|---|
| `$x^2$` en una línea sin cursor | existe `.katex` dentro de `.cm-editor` |
| Cursor en la línea de `$x^2$` | se ve `$x^2$` crudo |
| Bloque ` ```mermaid ` sin cursor | existe un `svg` dentro de `.cm-editor` |
| Cursor en el bloque mermaid | se ve el código mermaid |
| Tabla de 2 columnas | las pipes no se ven; las dos celdas de una fila quedan en la misma línea visual |
| `![alt](foto.png)` | existe `.cm-lp-image` con el texto `alt` y `title="foto.png"` |
| KaTeX que no carga | el bloque muestra el fuente, sin excepción en la consola |
| Mermaid con sintaxis inválida | muestra el fuente, sin excepción |

- [ ] **Step 2: Correrlo y ver que falla**

Run: `node scripts/check-live-editor.cjs`
Expected: FAIL en el caso de KaTeX.

- [ ] **Step 3: Refactorizar `preview-bridge.mjs`**

Hoy escanea `document`. Partir `renderMath` y `renderMermaid` en dos funciones por nodo, `renderMathInto(el, source, isDisplay)` y `renderMermaidInto(el, source)`, y dejar que las funciones que escanean el documento las llamen. Mantener `loadKatex` y `loadMermaid` tal como están: son la razón por la que los 5 MB no se descargan al arrancar.

- [ ] **Step 4: Los widgets en `live-preview.mjs`**

Una clase por widget, todas `WidgetType`. La forma, con `MathWidget` como ejemplo:

```js
class MathWidget extends WidgetType {
  constructor(source, isDisplay) { /* guardar source e isDisplay */ }
  eq(other) { /* true si source e isDisplay coinciden */ }
  toDOM() {
    const el = document.createElement(this.isDisplay ? "div" : "span");
    window.__codedocs_renderMathInto(el, this.source, this.isDisplay);
    return el;
  }
  ignoreEvent() { return false; }
}
```

`eq` es lo que impide que CodeMirror destruya y reconstruya el widget en cada transacción: si el contenido no cambió, `eq` devuelve `true` y el nodo se reutiliza. Es lo que tiene controlado el riesgo 2 del spec (re-renderizar Mermaid en cada tecla).

`ignoreEvent() { return false; }` en los widgets de math es obligatorio: sin él, hacer clic o seleccionar dentro del widget no llega a CodeMirror y el caret se pierde.

Para tabla: ocultar cada pipe y `Decoration.line({ class: "cm-lp-table-row" })` en las líneas de tabla, con `display: flex` y `gap` en el CSS. No es un `<table>` real, y así queda escrito en el CSS.

Para imagen: `ImageWidget` con un `<span class="cm-lp-image">` que muestra el alt y lleva la ruta en el `title`.

- [ ] **Step 5: Correr los tests y ver que pasan**

Run: `npm run build && node scripts/check-live-editor.cjs`
Expected: PASS.

- [ ] **Step 6: Commitear**

```bash
git add js/ scripts/
git commit -m "feat: widgets de math, Mermaid, tabla e imagen dentro del editor"
```

---

### Task 5: Ayudas de escritura

**Files:**
- Create: `js/markdown-input.mjs`
- Modify: `js/codemirror-bridge.mjs`, `Trunk.toml`, `package.json`, `index.html`, `scripts/check-bridge-contract.mjs`

**Interfaces:**
- Consumes: nada de otras tareas.
- Produces: `export function markdownInputExtension()` que devuelve un keymap de CodeMirror.

- [ ] **Step 1: Escribir los tests que fallan**

Agregar a `scripts/check-live-editor.cjs`:

| Caso | Entrada | Aserción |
|---|---|---|
| Continúa lista con `-` | `- a` + Enter | queda `- a\n- ` |
| Continúa lista numerada | `1. a` + Enter | queda `1. a\n2. ` |
| Continúa tarea | `- [x] a` + Enter | queda `- [x] a\n- [ ] ` |
| Sale de la lista | `- ` + Enter | queda una línea vacía, sin marcador |
| Tab indienta | `- a` con el cursor en la línea + Tab | queda `  - a` |
| Shift-Tab desindienta | `  - a` + Shift+Tab | queda `- a` |
| Tab fuera de lista | `texto` + Tab | quedan dos espacios |
| Regla de tipeo | `- ` al inicio de línea vacía | se convierte en marcador de lista |
| Regla de tarea | `- [ ] ` al inicio | queda `- [ ] ` |
| **Enter en bloque de código** | cursor dentro de un bloque cercado + Enter | se inserta un salto literal, el bloque no se parte |
| **Enter en cita** | cursor en una línea `> a` + Enter | se inserta el `> ` en la línea nueva |

Los dos últimos marcados en negrita son Review Focus 4.

- [ ] **Step 2: Correrlo y ver que falla**

Run: `node scripts/check-live-editor.cjs`
Expected: FAIL en el primer caso.

- [ ] **Step 3: Implementar `markdown-input.mjs`**

Un keymap con `prec: "highest"` para ganarle a `basicSetup`:

- **`Enter`**: si el cursor está en un `CodeBlock`, no interceptar, que `basicSetup` haga su comportamiento por defecto. Si está en un `ListItem`, calcular el marcador con `^(\s*)([-*+]|\d+\.)(\s+\[[ xX]\]\s+)?` e insertar `\n` más el marcador, con la numeración incrementada y la casilla desmarcada. Si el ítem no tiene texto después del marcador, sacar el marcador y salir de la lista en vez de crear uno vacío.
- **`Tab` / `Shift-Tab`**: dentro de una lista, indentar o desindentar el prefijo de espacios; fuera de una lista, insertar dos espacios.
- **`inputHandler`**: regex sobre el texto insertado en la línea actual. `- `, `* `, `+ ` al principio de una línea vacía ⇒ marcador de lista. `1. ` ⇒ ítem numerado. `- [ ] ` ⇒ tarea. Sólo dispara cuando el cursor está al final de lo insertado, para no secuestrar la escritura en el medio de una línea.

Nada de cierre automático de delimitadores: el usuario escribe markdown con los delimitadores y cerrarlos solos estorba (decisión del spec).

- [ ] **Step 4: Cablear el bundle**

`build:input` en `package.json`, `[[hooks]]` de `pre_build` en `Trunk.toml`, `./public/markdown-input.js` en `watch.ignore`, `<script src="/public/markdown-input.js"></script>` en `index.html`, y la entrada en `JS_FILES` de `check-bridge-contract.mjs`.

- [ ] **Step 5: Correr los tests y ver que pasan**

Run: `npm run build && node scripts/check-live-editor.cjs`
Expected: PASS en los once casos.

- [ ] **Step 6: Commitear**

```bash
git add js/ package.json Trunk.toml index.html scripts/
git commit -m "feat: continuacion de listas y tabulacion en el editor"
```

---

### Task 6: Borrar los modos y el render por tecla

**Files:**
- Modify: `src/state.rs`, `src/components/editor/mod.rs`, `src/components/editor/codemirror.rs`, `src/components/layout.rs`, `src/shortcuts.rs`, `src/utils/markdown.rs`, `package.json`, `README.md`

**Interfaces:**
- Consumes: `__codedocs_setSourceVisible(bool)` (Task 2), `EditorCommand` (Task 3).
- Produces:
  ```rust
  // src/state.rs
  pub source_mode: RwSignal<bool>,   // reemplaza a view_mode: RwSignal<ViewMode>
  ```
  `EditorCommand` queda con `Save, Bold, Italic, InlineCode, Strikethrough, Link, Focus, ToggleSource, NewFile`. Desaparecen `ViewRaw`, `ViewSplit`, `ViewFormatted`, `ToggleView`, y con ellos `needs_editor()` e `is_editing()`.

- [ ] **Step 1: Conectar los tests del crate frontend a `npm run verify`**

Hallazgo previo a este task: `npm run verify` corre `test:core` pero **nunca** `cargo test -p codedocs-ui`, así que los 23 tests unitarios del frontend se compilaban y nunca se ejecutaban. Compilados y corridos a mano pasan los 23, con dos warnings de imports sin usar que nadie veía porque tampoco se compilaba el target de tests.

En `package.json`, agregar:

```json
"test:ui": "cargo test -p codedocs-ui"
```

y sumar `&& npm run test:ui` a `verify`, después de `npm run test:core`.

De paso, limpiar los dos warnings que aparecen apenas se los ve: el `pub use codedocs_core::markdown::slugify;` sin usar en `src/components/sidebar/outline.rs:87`, y el `use super::*;` dentro de su `mod tests`.

- [ ] **Step 2: Escribir el test que falla**

En `src/shortcuts.rs`, reemplazar `editing_commands_require_an_editor` (que prueba un método que este task borra) por:

```rust
#[test]
fn source_toggle_is_bound_to_ctrl_slash() {
    assert_eq!(shortcut_for(EditorCommand::ToggleSource), Some((true, "/")));
}
```

No hace falta un test para «los modos ya no existen»: si alguien los reintroduce y los usa, el archivo no compila; si los reintroduce sin usar, el clippy los marca como código muerto. Inventar un test que cubra ese caso agrega mantenimiento sin cubrir nada.

- [ ] **Step 3: Correrlo y ver que falla**

Run: `npm run verify`
Expected: FAIL en `source_toggle_is_bound_to_ctrl_slash` porque `EditorCommand::ToggleSource` no existe todavía.

- [ ] **Step 4: Cambiar el estado**

En `src/state.rs`: borrar el enum `ViewMode` y el campo `view_mode`; agregar `source_mode: RwSignal<bool>` inicializado en `false`.

- [ ] **Step 5: Cambiar los comandos**

En `codemirror.rs`: borrar las cuatro variantes de modo, `needs_editor` e `is_editing`. Agregar `ToggleSource`, cuyo `run` hace `state.source_mode.update(|b| !*b)`. `shortcut_for` le asigna `(true, "/")`.

En `shortcuts.rs`: reemplazar las cuatro entradas de modo por `EditorCommand::ToggleSource`. Los tests `every_command_has_a_shortcut` y `shortcuts_are_unique` siguen cubriendo la tabla.

- [ ] **Step 6: Empujar `source_mode` al editor**

En `codemirror.rs`, un `Effect` más:

```rust
Effect::new(move |_| {
    let visible = state.source_mode.get();
    if mounted.get_untracked() {
        set_source_visible(visible);
    }
});
```

El toggle vive en Rust, no en JavaScript: una sola fuente de verdad, y el estado del header o del status bar lo pueden leer sin preguntar al editor.

- [ ] **Step 7: Borrar la preview**

En `editor/mod.rs`: borrar el componente `Preview`, el `extern "C"` de `__codedocs_render_enhancements_async`, `yield_to_browser`, y el parámetro `preview_html`. `EditorPane` queda con una sola rama, sin `match` sobre `view_mode`.

En `layout.rs`: borrar el `use` de `render_markdown`, el `RwSignal` de `preview_html`, su creación y el `preview_html.set(...)` del `Effect`. El `Effect` queda sólo con `state.headings.set(extract_headings(&content))`.

En `utils/markdown.rs`: sacar el re-export de `render_markdown`. `outline.rs` lo usa con ruta completa (`codedocs_core::render_markdown`), así que no se rompe.

- [ ] **Step 8: Documentar el atajo**

En `README.md`, reemplazar las líneas 35-36 (los modos `Ctrl+1/2/3` y el toggle `Ctrl+0`) por la descripción del modo vivo y del toggle `Ctrl+/`.

- [ ] **Step 9: Verificar que todo pasa**

Run: `npm run verify`
Expected: verde entero, con los 23 tests del frontend corriendo ahora.

- [ ] **Step 10: Commitear**

```bash
git add src/ README.md package.json
git commit -m "refactor: un solo modo de edicion, se borra la preview de solo lectura"
```

---

### Task 7: Verificación de navegador y contratos

**Files:**
- Modify: `scripts/check-preview-render.cjs`, `package.json`, `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: `window.__codedocs_renderMathInto`, `window.__codedocs_renderMermaidInto` (Task 4).
- Produces: `npm run verify:browser` que cubre también el editor en vivo.

- [ ] **Step 1: Adaptar `check-preview-render.cjs`**

Hoy busca nodos KaTeX y Mermaid en el DOM de la preview, que ya no existe. Reescribirlo para que monte un documento con math y un diagrama, los escriba en el editor con el caret fuera de la línea, y verifique que `.katex` y el `svg` del mermaid aparecen **dentro de `.cm-editor`**.

- [ ] **Step 2: Agregar el editor a `verify:browser`**

```json
"verify:browser": "npm run build && npm run check:glue && npm run check:csp && npm run check:ipc && npm run check:preview && npm run check:live"
```

- [ ] **Step 3: Correr la suite completa**

Run: `npm run verify:browser`
Expected: todos los checks en PASS.

- [ ] **Step 4: Agregar el paso a CI**

En `.github/workflows/ci.yml`, en el job que ya corre la verificación de navegador, agregar `node scripts/check-live-editor.cjs` después de `check-preview-render.cjs`. `PLAYWRIGHT_BROWSERS_PATH` ya está configurado.

- [ ] **Step 5: Commitear**

```bash
git add scripts/ package.json .github/
git commit -m "test: verificacion del editor en vivo en navegador y CI"
```

---

## Cierre

- [ ] **Step 1: Correr todo**

Run: `npm run verify && npm run verify:browser`
Expected: verde entero.

- [ ] **Step 2: Anotar el límite conocido**

Al final de `README.md`, en la sección de limitaciones: las imágenes markdown no cargan por el CSP (`img-src 'self' data: blob:`), en el editor se ve el alt con la ruta en el tooltip, y la vista en vivo se verificó en Chromium mientras la app corre sobre WebKitGTK en Linux.

- [ ] **Step 3: Commitear**

```bash
git add README.md
git commit -m "docs: limita el alcance de la vista en vivo"
```