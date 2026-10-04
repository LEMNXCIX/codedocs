# CodeDocs

Editor de Markdown de escritorio, al estilo Typora, construido con **Tauri 2 +
Leptos 0.8 (CSR) + CodeMirror 6**. CodeDocs abre una carpeta como proyecto, muestra
el árbol de `.md` en el sidebar y los renderiza en un preview en vivo con tablas,
notas al pie, ~~tachado~~, listas de tareas, matemáticas con KaTeX y diagramas
Mermaid. Todo el backend es Rust; no hay servidor, ni cuenta, ni telemetría.

El editor abre **una carpeta a la vez**, no un archivo suelto: el documento, el
índice de títulos y el vigilante de archivos viven dentro del proyecto abierto.

## Qué hace hoy

- **Selector de carpeta de proyecto** (diálogo nativo) y modo demo en el navegador.
- **Árbol recursivo de Markdown** — carpetas primero, solo `.md`, carpetas
  ocultas y de vendor (`node_modules`, `target`, `.venv`, `dist`, …) ignoradas.
  El recorrido está **acotado** (tope de entradas y de profundidad, protección
  contra ciclos de symlinks) y corre fuera del hilo de la UI, así que abrir una
  carpeta grande no cuelga la app.
- **Abrir, guardar, crear, eliminar y renombrar** archivos. La escritura es
  atómica (archivo temporal + `rename`) y hay un tope de 5 MB por documento.
- **Editor CodeMirror 6** con resaltado de Markdown y resaltado del lenguaje
  dentro de los bloques de código.
- **Preview en vivo** con tablas GFM, notas al pie, tachado, listas de tareas y
  puntuación tipográfica. KaTeX y Mermaid se **cargan bajo demanda**: un
  documento sin matemáticas ni diagramas no los descarga nunca.
- **Panel de contenido (outline)** con los encabezados del documento en vivo; al
  hacer click se desplaza el preview hasta esa sección.
- **Auto-guardado con rebote** de 1,5 s, más `Ctrl+S` y un botón de guardar.
- **Indicador de estado de guardado** en el header y en la barra de estado:
  `Sin guardar` / `Guardando…` / `Guardado` / `Error al guardar`.
- **Detección de cambios externos** con `notify`: si editás el archivo desde otro
  programa, el preview se recarga solo (siempre que no tengas cambios sin
  guardar).
- **Editor en vivo**: el formato se muestra directamente mientras escribís, sin
  panel de preview separado. `Ctrl+/` alterna la vista de la fuente Markdown
  (con el estilo conservado).
- **Tema claro/oscuro** con doble clic en el logo.
- **Conteo de palabras y caracteres** en la barra de estado.
- **Atajos de teclado** declarados en una sola tabla (`src/shortcuts.rs`).
- Sidebar **redimensionable** arrastrando el divisor.

Lo que **no** existe todavía está en [`docs/ROADMAP.md`](docs/ROADMAP.md), con
cada casilla verificada contra el código.

## Requisitos

- **Rust** estable con el target WASM: `rustup target add wasm32-unknown-unknown`.
- **Node.js** y **Trunk** (`cargo install trunk`).
- **Dependencias de sistema de Tauri v2 en Linux** (Debian/Ubuntu):

  ```bash
  sudo apt update
  sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
      librsvg2-dev build-essential curl wget file libssl-dev libxdo-dev
  ```

  `libgtk-3-dev` llega como dependencia de `libwebkit2gtk-4.1-dev`; se lista
  explícitamente porque `cargo check -p codedocs` lo necesita igual.
  En macOS hace falta Xcode; en Windows, *Microsoft C++ Build Tools* (opción
  *Desktop development with C++*) y el runtime de Edge WebView2.

## Desarrollo

```bash
npm install              # dependencias de JS (CodeMirror, KaTeX, Mermaid, esbuild…)

npm run tauri            # app de escritorio (equivale a `cargo tauri dev`)
npm run dev              # solo el frontend en el navegador (trunk serve)

npm run build            # build de producción del frontend (trunk build --release)
npm run build:cm         # recompila solo el puente de CodeMirror
npm run build:preview    # recompila solo el puente de KaTeX/Mermaid
```

Los dos `build:*` existen porque los bundles de `js/*.mjs` **no** se versionan
(los regenera el hook `pre_build` de `Trunk.toml`). Si tocás un `.mjs`, corré el
script correspondiente o compilá una vez; si no, tu cambio no se ve.

### Verificación

```bash
cargo test  -p codedocs-core                                    # 68 tests
cargo clippy -p codedocs-core --all-targets -- -D warnings
cargo fmt --all -- --check
cargo check --target wasm32-unknown-unknown                     # frontend
cargo check -p codedocs                                          # backend Tauri
```

`cargo check -p codedocs` requiere las dependencias de sistema de arriba. Lo
mismo hace el job `tauri` de CI, que instala los paquetes de Linux
explícitamente.

## Estructura

```
codedocs/
├── codedocs-core/         # lógica compartida — sin Tauri, sin Leptos
│   └── src/
│       ├── lib.rs
│       ├── types.rs       # FileEntry
│       ├── markdown/      # render_markdown, extract_headings, math, sanitize
│       ├── path_guard.rs  # Workspace: el límite del proyecto
│       └── tree.rs        # recorrido acotado del árbol de archivos
├── src/                   # frontend Leptos (WASM)
│   ├── main.rs
│   ├── app.rs
│   ├── state.rs           # EditorState, SaveState
│   ├── actions.rs         # operaciones: abrir, guardar, crear, borrar, renombrar
│   ├── shortcuts.rs       # tabla de atajos
│   ├── watch.rs           # suscripción al vigilante de archivos
│   ├── components/        # layout, header, sidebar/, editor/, modals/, status_bar.rs
│   └── utils/             # env.rs, markdown.rs (reexport), tauri_bridge.rs
├── src-tauri/             # backend Tauri
│   ├── src/lib.rs         # registro de comandos
│   ├── src/main.rs
│   ├── src/commands.rs    # toda la superficie de archivos
│   ├── src/state.rs       # Workspace abierto + watcher + log de escrituras
│   └── tauri.conf.json
├── js/                    # puentes JS ↔ Leptos (CodeMirror, KaTeX/Mermaid)
├── public/                # íconos y assets vendor de KaTeX/Mermaid
├── docs/ROADMAP.md
├── Cargo.toml             # paquete codedocs-ui + raíz del workspace
├── Trunk.toml             # hooks de tailwind y esbuild
└── package.json
```

### Por qué existe `codedocs-core`

Los dos lados de la frontera IPC necesitan *acordar* cosas: la forma de una
entrada del árbol, cómo se renderiza Markdown y qué rutas se pueden tocar. Si eso
viviera en `src-tauri`, el frontend no podría compartirlo (no compila para
WASM con Tauri), y si viviera en `src/`, el backend tendría que duplicarlo — que
fue exactamente el bug que había: dos `render_markdown` que podían divergir.

`codedocs-core` es Rust pelado, sin Tauri ni Leptos, así que compila para ambos
targets **y** se testea con un `cargo test` normal en el host. Ahí viven los 68
tests: sanitización de Markdown, detección de math, guardián de rutas y límites
del recorrido. `src/utils/markdown.rs` y `src/types.rs` son reexports delgados
para no romper los call sites.

## Notas de arquitectura

### Markdown: allowlist por construcción

El preview renderiza **archivos que el usuario abrió**, o sea contenido no
confiable, y Markdown permite HTML crudo. `pulldown-cmark` lo pasa tal cual, así
que renderizar un documento hostil a mano equivale a ejecutar JavaScript dentro
de un webview de Tauri que tiene `__TAURI__` en `window`.

En vez de limpiar el HTML *después* (frágil: exige que un parser de HTML sea
correcto), `codedocs-core` construye la salida desde una allowlist:

1. Todo `Event::Html` / `Event::InlineHtml` producido por el **documento** se
   descarta. El HTML crudo del archivo nunca llega a la salida.
2. Todo `Event::Html` que sí sale lo generó el propio renderer (los envoltorios
   de math y Mermaid, y las etiquetas `<hN id=…>` del outline).
3. Las URLs de enlaces e imágenes se validan por esquema (`is_safe_url`):
   `http`, `https`, `mailto`, `tel` y rutas relativas. `javascript:`,
   `data:` o `file://` se neutralizan — se conserva el texto del enlace pero no
   el destino.

El texto del documento siempre viaja como `Event::Text`, que `push_html` escapa.
Del lado del navegador, `js/preview-bridge.mjs` lee el TeX y el código Mermaid
como `textContent` y los escribe con construcción de DOM o con el SVG que genera
Mermaid — nunca con concatenación de `innerHTML`.

> La CSP de Tauri sigue en `null` en `tauri.conf.json`: la defensa actual es el
> modelo de allowlist de arriba, no la CSP. Está anotado en el roadmap.

### Guardián de rutas: `Workspace`

Todos los comandos de archivos son alcanzables desde el JavaScript del webview, así
que un comando que confía en su argumento de ruta es una primitiva de
lectura/escritura/borrado arbitrario esperando un XSS. `Workspace` es el único
punto de paso:

- La raíz se canonicaliza al abrir la carpeta, y **toda** ruta se canonicaliza
  *antes* de comprobar la contención — un symlink dentro del proyecto que
  apunte a `/etc` se rechaza, no se sigue.
- Crear y renombrar validan el nombre nuevo: nombre pelado, sin separadores ni
  `..`, extensión `.md`. Por eso `rename_file("../evil.md", …)` no escapa.
- El vigilante de archivos vive en estado gestionado y se **reemplaza** al
  cambiar de carpeta, nunca se acumula.

## Contribuir

Véase [CONTRIBUTING.md](CONTRIBUTING.md).

## Licencia

[MIT](LICENSE) © 2026 CodeDocs contributors.