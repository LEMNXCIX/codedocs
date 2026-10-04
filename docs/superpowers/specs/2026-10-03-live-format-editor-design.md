# Editor en formato vivo — diseño

Fecha: 2026-10-03
Estado: aprobado en conversación, pendiente de revisión de este documento

## El problema

CodeDocs tiene tres modos: `Raw` (fuente), `Formatted` (preview de solo lectura) y
`Split`. Escribir en `Raw` es escribir markdown a ciegas; leer en `Formatted` no
permite editar. Quien escribe notas pasa el tiempo mirando sintaxis.

El pedido: poder seguir escribiendo y ver el formato aplicado en vivo, como en
Obsidian o Notion.

## Decisiones tomadas

| Decisión | Elegida | Descartada y por qué |
|---|---|---|
| Modelo de edición | Decoraciones sobre el markdown (Obsidian) | `contenteditable` puro (Notion): requiere un serializador DOM→markdown propio; listas anidadas, tablas y footnotes no sobreviven la ida y vuelta y el archivo se corrompe |
| Alcance v1 | Bloques + inline + tablas + math + Mermaid | — |
| Modos | Uno solo (vivo) con toggle de fuente | Split y preview de solo lectura se eliminan |
| Revelado | Línea del cursor siempre en crudo + toggle global | Sólo toggle global: obliga a ver los `**` mientras editás |
| Ayudas | Continuación de listas y Tab/Shift-Tab | Cierre automático de delimitadores: molesta a quien ya escribe markdown |
| Motor de decoraciones | Tramos calculados por el parser de Rust con offsets | Árbol de sintaxis de CM6 (segundo parser, diverge) y scanner de regex (frágil con énfasis anidados) |

## Arquitectura

Tres piezas, con una frontera clara entre ellas.

### 1. `codedocs-core/src/markdown/live.rs` — calcula los tramos

Puro, sin Tauri ni webkit, testeable con `cargo test -p codedocs-core`.

```rust
/// Qué hacer con un tramo del documento mientras se edita. El discriminante
/// es lo que JavaScript recibe, así que los valores están fijados.
#[repr(u8)]
pub enum SpanTag {
    Hide = 0,
    Strong = 1,
    Emphasis = 2,
    Strikethrough = 3,
    InlineCode = 4,
    LinkText = 5,
    ImageAlt = 6,
    Heading1 = 7,   // .. Heading6 = 12
    Quote = 13,
    ListMarker = 14,
    Fence = 15,
    TablePipe = 16,
    TableDelimiter = 17,
    MathInline = 18,
    MathDisplay = 19,
    MermaidBlock = 20,
}

/// Tramos planos, listos para cruzar a JavaScript.
///
/// `ranges` son offsets en **unidades UTF-16** (lo que cuenta CodeMirror), no
/// en bytes (lo que cuenta `pulldown-cmark`).
pub struct LiveSpans {
    /// `[from0, to0, from1, to1, ...]`
    pub ranges: Vec<u32>,
    /// Un `SpanTag` por tramo. Se serializa al cruce como su discriminante
    /// `#[repr(u8)]`, para que JavaScript no dependa de los nombres.
    pub tags: Vec<SpanTag>,
}

pub fn live_spans(content: &str) -> LiveSpans
```

Cruza a JS como dos `TypedArray` (`Uint32Array` + `Uint8Array`), no como JSON.

### Tramos que se solapan

El parser puede emitir tramos anidados que se toquen (un marcador de cita y el
de una lista dentro de la misma línea). `live_spans` los ordena por `from` y
resuelve la colisión **quedándose con el externo**: se descarta el interno. El
test de invariantes afirma que la salida no tiene solapamientos, así que si el
parser produce un caso que la regla no cubre, el test lo delata en vez de
dejarlo llegar a CodeMirror, que rechaza decoraciones superpuestas.

### La regla central

`pulldown-cmark` con `into_offset_iter()` devuelve el rango exacto de cada
evento. De ahí sale una regla que cubre casi todas las construcciones:

> **El marcador de una construcción es la parte de su rango que no está
> cubierta por sus hijos.**

- `# Título` — rango del heading `0..9`, hijo `Título` es `2..9` ⇒ marcador `0..2`.
- `**negrita**` — rango `0..10`, hijo `1..7` ⇒ marcadores `0..1` y `7..10`.
- `[etiqueta](url)` — el rango completo, hijo = la etiqueta ⇒ se oculta `[` y `](url)`.
- `## Título {#id}` — el `{#id}` no es hijo, así que cae en la misma regla y se
  oculta gratis.

No hay un caso por construcción. Si mañana se agrega algo al renderer, aparece
en vivo sin tocar el editor.

### 2. Conversión de offsets — la parte peligrosa

CodeMirror cuenta en UTF-16, `pulldown-cmark` en bytes. Con `ñáé` o un emoji, un
byte no es un carácter: `"# ñ😀"` son 8 bytes pero 5 unidades UTF-16.

Se arma un índice de inicios de línea y se hace búsqueda binaria por tramo.
Función propia, con tests que usan acentos y emojis explícitamente.

### 3. `js/live-preview.mjs` — el `ViewPlugin` de CodeMirror

Traduce los tramos a `Decoration.replace` (ocultar), `Decoration.mark`
(estilizar) y widgets. Recalcula por transacción y por cambio de selección.

## Flujo por tecla

```
tecla → transacción CM6 → docChanged
  ├→ onChange → state.content → autosave (sin cambios, ya funciona)
  └→ ViewPlugin → wasm: live_spans(doc) → Uint32Array + Uint8Array
                → RangeSetBuilder → decoraciones
```

El markdown sigue siendo el documento de CodeMirror. **No hay serializador
DOM→markdown**, así que el archivo no se puede corromper: lo que se escribe es
lo que se guarda, byte a byte.

## Regla de revelado

Un tramo se oculta salvo que:

- su rango toque la **línea del cursor**,
- esté dentro de la **selección**, o
- el toggle global de fuente esté activo.

Es decir: nunca se edita a ciegas. La línea en la que se está escribiendo
siempre se ve en markdown crudo; el resto, renderizado.

## Widgets

| Construcción | Cómo se ve | Con el cursor encima |
|---|---|---|
| Bloque de código | Vallas ocultas, resaltado de sintaxis normal de CM6 | Se ven las vallas y el lenguaje |
| `$inline$` | KaTeX vía `Decoration.replace` | Crudo |
| `$$display$$` | KaTeX en bloque | Crudo |
| ```` ```mermaid ```` | Diagrama renderizado | El código Mermaid |
| Tabla | Pipes ocultos, celdas alineadas en grilla de líneas | La tabla en crudo |
| Imagen | Placeholder con el alt y la ruta en el tooltip | `![alt](ruta)` |

KaTeX y Mermaid conservan los cargadores perezosos y cacheados que ya existen
(`loadKatex` / `loadMermaid`), así que los ~5 MB se descargan sólo si el
documento tiene math o un diagrama.

`preview-bridge.mjs` deja de escanear `document` y pasa a exponer renderizadores
por nodo (`renderMathInto`, `renderMermaidInto`), reutilizados por los widgets.

### Límites asumidos

1. **Las tablas no son un `<table>` real.** Son líneas con las pipes ocultas y
   celdas alineadas. Un `<table>` de verdad dentro de CodeMirror pelea con el
   cursor y con la selección por celda; no vale la complejidad.

2. **Las imágenes no cargan, y no es culpa de este cambio.** El CSP tiene
   `img-src 'self' data: blob:`, así que un `![](foto.png)` relativo al
   workspace no resuelve. Ya está roto hoy en la preview; no se empeora. Se ve
   el alt con la ruta en el tooltip. Arreglarlo exige tocar el CSP o usar
   `convertFileSrc`, que es una decisión de seguridad aparte y fuera de alcance.

## Ayudas de escritura

Sólo lo pedido:

- **Enter** en un ítem de lista → crea el siguiente con el mismo marcador
  (`- `, `* `, `1. ` → `2. `, `- [ ] ` → `- [ ] ` sin marcar).
- **Enter** en un ítem vacío → saca el marcador y sale de la lista.
- **Tab / Shift-Tab** dentro de una lista → indienta y desindienta un nivel.
  Fuera de una lista → dos espacios.
- **Reglas de tipeo**: escribir `- `, `* `, `+ ` o `1. ` al principio de una
  línea vacía arma la lista; `- [ ] ` arma la lista de tareas.

## Qué se borra

Este cambio **borra trabajo en vez de agregar**:

- `ViewMode::{Raw, Formatted, Split}` y `state.view_mode` → un solo
  `state.source_mode: RwSignal<bool>`.
- El componente `Preview` y la señal `preview_html`.
- El `Effect` de `layout.rs` que llama a `render_markdown(&content)` **en cada
  tecla**. Ese render HTML completo ya no lo dibuja nadie. Es la corrección de
  performance más grande del cambio.
- `EditorCommand::needs_editor` / `is_editing`: con un editor siempre montado
  dejan de tener sentido.

### Botones de modo

Corrección respecto de una versión anterior de este documento: **hoy no hay
botones de modo**. Los modos sólo se cambian por atajo (`Ctrl+0/1/2/3`) y el
header no los expone. El toggle de fuente queda sólo por teclado (`Ctrl+/`) y se
documenta en el README junto con el resto de la tabla de atajos. No se agrega
botón nuevo: no hay UI de modos que reemplazarla, y agregarla sería alcance
extra.
- `src/utils/markdown.rs` pierde el re-export de `render_markdown`.

### Atajos

`Ctrl+0/1/2/3` (modos) se reemplazan por **uno solo**: `Ctrl+/`, que alterna la
fuente global. `Ctrl+S`, `Ctrl+B/I/E`, `Ctrl+K` y `Ctrl+N` quedan igual.

`Ctrl+/` se registra en el keymap de CodeMirror con `preventDefault`, así que
pisa el "comentar" que algunos navegadores asignan a esa combinación; es
intencional.

### Consecuencia asumida

Al caer la preview de solo lectura, `render_markdown` deja de usarse dentro de la
app: queda en `codedocs-core` con sus tests de seguridad y el ejemplo
`render.rs`. Se conserva a propósito — es la implementación de referencia con la
que `live_spans` comparte parser, y `live_spans` reutiliza `parser_options()`.
Si la preview no vuelve nunca, es código muerto y se revisa en otro momento.

## Archivos tocados

**Nuevos**
- `codedocs-core/src/markdown/live.rs` — cálculo de tramos
- `codedocs-core/tests/live_invariants.rs` — invariantes sobre cualquier documento
- `codedocs-core/examples/live.rs` — vuelca tramos de un archivo real
- `js/live-preview.mjs` — `ViewPlugin` + decoraciones
- `scripts/check-live-editor.cjs` — verificación en navegador real

**Modificados**
- `codedocs-core/src/markdown/mod.rs` — expone `live_spans`, comparte `parser_options`
- `codedocs-core/src/lib.rs` — export
- `js/codemirror-bridge.mjs` — cablea las extensiones nuevas
- `js/preview-bridge.mjs` — renderizadores por nodo en vez de escaneo de `document`
- `src/state.rs` — `source_mode` en lugar de `view_mode`
- `src/components/editor/mod.rs` — sin `Preview`
- `src/components/layout.rs` — sin `preview_html` ni el render por tecla
- `src/components/header.rs` — toggle de fuente
- `src/components/editor/codemirror.rs` — comandos y atajos
- `src/shortcuts.rs` — tabla de atajos
- `input.css` — clases `.cm-lp-*` con variantes `dark:`
- `scripts/check-bridge-contract.mjs`, `scripts/check-preview-render.cjs` — adaptados

## Testing

**`cargo test -p codedocs-core`** (sin webkit; es la cobertura real)

- `live_spans` por construcción: heading, strong, emphasis, inline code,
  strikethrough, link, image, tabla, math, bloque de código, cita.
- Casos degenerados: `**` solo, `[]()` vacío, siete `#`, `#` suelto, énfasis
  anidado `**a *b* c**`. Ninguno puede entrar en panic ni invertir un rango.
- Test de UTF-16: `"# ñ😀"` debe dar un tramo oculto `0..2` en UTF-16, no
  `0..3` en bytes.
- Invariantes sobre cualquier documento: `from <= to`, tramos ordenados por
  `from`, sin solapamientos (tras la resolución externa-interna), ninguno fuera
  del documento.

**`scripts/check-live-editor.cjs`** (navegador real; lo único que verifica el cursor)

- Escribir `**hola**` ⇒ el contenido es exactamente `**hola**` y la negrita
  está pintada.
- El `**` no está visible; con el cursor en la línea, vuelve a estar.
- `Ctrl+/` muestra toda la fuente.
- Enter en lista, Enter en ítem vacío, Tab.
- Math y Mermaid renderizan; guardar escribe exactamente lo tipeado.
- **Perf:** documento de 5 000 líneas. Se mide la mediana de 20 pulsaciones de
  tecla, desde el `dispatch` de la transacción hasta el callback de
  `requestAnimationFrame` siguiente, y el presupuesto es **< 50 ms**. Si lo
  supera, recién ahí se optimiza a sólo-viewport.

## Orden de implementación

**Fase 0 — spike del cursor (bloqueante).** Antes de construir nada, un script
de ~30 líneas que esconda `**` con `Decoration.replace({})` y verifique dónde
queda el caret al tipear y al mover las flechas, comparando con `getContent()`.
Si no se banca: probar `Decoration.mark` con `display: none`; si tampoco, la
regla del cursor-línea cubre el caso. Es lo único que podría obligar a cambiar
de enfoque, así que va primero.

**Fase 1** `live_spans` + conversión UTF-16 + tests en `codedocs-core`.
**Fase 2** `js/live-preview.mjs`: ocultar y estilizar, más el toggle de fuente.
**Fase 3** widgets: math, Mermaid, tabla, imagen.
**Fase 4** ayudas de escritura.
**Fase 5** borrar modos, `Preview`, `preview_html` y el render por tecla.
**Fase 6** actualizar scripts de contrato y `check-live-editor.cjs`.

## Riesgos

1. **El cursor y el texto oculto.** Es el riesgo real: esconder `**` puede
   hacer que el caret salte al pasar con la flecha, o que no se pueda colocar
   entre dos marcadores. Mitigado por la Fase 0.

2. **Re-renderizar Mermaid en cada tecla.** Sería catastróficamente lento. Los
   widgets sólo se renderizan cuando el bloque **no** está en la línea del
   cursor, y se cachean por hash del contenido: si el diagrama no cambió, no se
   vuelve a renderizar.

3. **Reparseo del documento entero por tecla.** Se mide con el test de 5 000
   líneas. Si pasa el presupuesto, no se hace nada.

4. **Modo oscuro en las decoraciones.** `oneDark` sólo estiliza el editor; las
   clases `.cm-lp-*` necesitan variantes `dark:` propias o la vista en vivo
   queda ilegible de noche.

5. **Chromium no es WebKitGTK.** En Linux la app corre sobre WebKitGTK y los
   tests corren en Chromium, así que el comportamiento del caret *puede*
   diferir. Limitación honesta y ya presente en toda la verificación del repo.

6. **Regresiones en lo que ya anda:** autosave, watcher, outline, atajos, tema.
   Ningún camino de escritura cambia; quedan cubiertos por `npm run verify` y la
   suite de navegador.

## Fuera de alcance

- Reparar la carga de imágenes (exige tocar el CSP).
- Un `<table>` real dentro del editor.
- Cierre automático de delimitadores.
- Colaboración / multiusuario.
- Vista de lectura.