# CodeDocs — Roadmap a Typora

> Cada casilla de este archivo se verificó contra el código antes de marcarla. Si
> encontrás una que miente, es un bug del documento: corregilo en el mismo PR.

---

## Fase 0.5 — Saneamiento (completado)

Ronda de trabajo sin features nuevas: seguridad, corrección de bugs que
provocaban cuelgues y pérdida de datos, fugas de memoria y limpieza
estructural. El detalle de cada punto está en el módulo o archivo donde vive.

### Seguridad

- **Sanitización del preview.** El renderer descarta todo `Event::Html` /
  `Event::InlineHtml` que venga del *documento*: el HTML crudo del archivo nunca
  llega a la salida. La allowlist es por construcción — lo único que se emite
  como HTML es lo que generó el propio renderer. El texto siempre viaja como
  `Event::Text`, que `push_html` escapa.
- **URLs con esquema no permitido neutralizadas.** `is_safe_url` acepta
  `http`/`https`/`mailto`/`tel` y rutas relativas; `javascript:`, `data:`,
  `file:` y los esquemas mal formados se rechazan conservando el texto del
  enlace pero descartando el destino.
- **Fin del sink XSS del fallback de Mermaid.** Cuando un diagrama no se puede
  renderizar, el código fuente se muestra con construcción de DOM
  (`textContent`), no con `innerHTML = "<pre>" + code + "</pre>"`.
- **Guardián de rutas (`Workspace`) en todos los comandos de archivos.** Cada
  comando canonicaliza su ruta *antes* de comprobar la contención en la raíz del
  proyecto, así que un symlink que apunta afuera se rechaza en vez de seguirse.
  Crear y renombrar validan el nombre nuevo (nombre pelado, sin separadores ni
  `..`, extensión `.md`).
- **Escritura atómica de archivos.** Temporal en el mismo directorio + `rename`,
  con `sync_all` antes del rename. Un `fs::write` directo trunca primero y un
  crash a mitad de escritura destruye el documento.
- **Tope de tamaño de documento** (5 MB) al leer y al escribir, para no colgar el
  webview con un archivo enorme.
- [ ] **CSP** — sigue en `null` en `src-tauri/tauri.conf.json`. La defensa actual
  es el modelo de allowlist del renderer, no la CSP. Activarla rompería
  `js/preview-bridge.mjs`, que inyecta KaTeX y Mermaid con
  `document.createElement('script')`, salvo que la allowlist lo contemplara.

### Corrección

- **Fin del loop infinito de auto-guardado.** El efecto de auto-guardado ahora
  compara `content` contra `last_saved`, y el backend filtra sus propias
  escrituras con `WriteLog` (ventana de 1,5 s). Antes el ciclo era
  guardar → vigilante → recargar → auto-guardar → guardar.
- **Token de carga al abrir un archivo.** Cada apertura toma un token y las
  respuestas viejas se descartan: hacer click rápido en dos archivos ya no puede
  dejar el contenido de A bajo el nombre de B (y el auto-guardado escribiendo uno
  sobre el otro).
- **Corrección de `extract_headings`.** El texto de un encabezado se acumula a lo
  largo de todo el encabezado (énfasis, código y enlaces incluidos) en vez de
  cortarse en el primer nodo de texto. Hay slugs únicos por documento, porque dos
  encabezados idénticos compartirían id y el outline saltaría siempre al primero.
- **Math que ya no rompe los bloques de código.** La detección `$…$` corre
  únicamente sobre eventos de texto en modo flujo, así que bloques de código y
  spans de código quedan estructuralmente fuera de su alcance.
- **Sin recompilación por nodo.** El detector de math es un escáner de caracteres
  escrito a mano (`split_math`), no una regex recompilada en cada pulsación.
- **Nombres de archivo nuevos únicos.** `unique_file_name` agrega un contador en
  vez de chocar siempre con `Nuevo_Documento.md`.
- **Errores al usuario.** Los fallos de guardar/crear/eliminar/renombrar ya no se
  pierden en la consola: salen como toast y como estado de guardado.

### Rendimiento / cuelgue

- **Recorrido del árbol acotado.** Ignora carpetas ocultas y de vendor
  (`.git`, `node_modules`, `target`, `dist`, `build`, `.venv`, `vendor`,
  `__pycache__`, `coverage`, `Pods`), tiene tope de entradas (8 000) y de
  profundidad (8) para la apertura interactiva, y lleva un conjunto de rutas
  canónicas ya visitadas para terminar ante un ciclo de symlinks. Este es el
  arreglo del cuelgue al abrir carpetas grandes: si el presupuesto se agota se
  informa al usuario en vez de truncar en silencio.
- **Recorrido fuera del hilo de la UI.** `list_markdown_files` corre el tree walk
  en el pool bloqueante de Tauri.
- **KaTeX y Mermaid con carga diferida.** `index.html` ya no los incluye; se
  inyectan la primera vez que un documento contiene math o un diagrama. Un
  documento sin ninguno de los dos no descarga los ~3,4 MB.
- **Preview parseado en un efecto**, no en el camino de render de cada signal
  no relacionado.

### Memoria / fugas

- **Timer de auto-guardado cancelable.** Usa `set_timeout_with_handle`, así que
  la pulsación siguiente cancela el guardado pendiente. Antes era
  `Closure::once(..).forget()`, que filtraba una copia completa del documento
  por pulsación.
- **`window_event_listener` con `remove()` en cleanup.** En atajos, watcher,
  redimensionado del sidebar y cierre de modales. Antes los handles se
  descartaban y quedaban adjuntos de por vida.
- **Vigilante de archivos liberable.** Se guarda el id devuelto por `listen()` y
  en el cleanup se llama a `unlisten()` y a `stop_watching()`, de modo que abrir
  una segunda carpeta no deja dos watchers peleándose por emitir.
- [ ] **`Callback` recreados en cada render.** Los `Callback::new(..)` en línea
  dentro de `view!` (`src/components/layout.rs`, `src/components/sidebar/mod.rs`)
  se siguen creando en cada render. No es una fuga grave porque el componente
  raíz casi no se re-renderiza, pero queda como deuda conocida.
- Nota: `src/watch.rs` sigue filtrando deliberadamente el `Closure<dyn Fn>` de
  Rust, que no es `Send` ni `Sync` y no puede entrar en `on_cleanup`. Es
  acotado (uno por carpeta abierta) y está comentado en el código; lo que importa
  —desvincular el listener de JS y soltar el watcher del backend— sí se hace.

### Arquitectura / limpieza

- **Crate nuevo `codedocs-core`.** Rust pelado, sin Tauri ni Leptos: compila
  para WASM y para nativo, y se testea con un `cargo test` normal en el host.
  Reúne `FileEntry`, el render de Markdown, el guardián de rutas y el recorrido
  del árbol — las cosas que ambos lados de la frontera IPC necesitan acordar.
- **`tauri_bridge` tipado.** Un wrapper por comando, con nombres de argumentos y
  deserialización en un solo lugar: renombrar un comando del lado de Rust pasa a
  ser un error de compilación y no un fallo silencioso en runtime.
- **Código muerto eliminado:** `generate_toc`, `greet`, `src-tauri/src/utils/md.rs`
  (con su `mod.rs`) y `src/components/ui.rs`.
- **Modales unificados.** Un `ModalShell` compartido por delete / rename / alert,
  con un único listener de Escape atado al ciclo de vida del componente.
- **Atajos de teclado como tabla.** `src/shortcuts.rs` recorre una lista de
  `EditorCommand`; cada comando declara su combinação en
  `src/components/editor/codemirror.rs`, con un test que verifica que no haya dos
  comandos con el mismo atajo.
- **Artefactos de build fuera del control de versiones.** `styles.css` y los dos
  bundles de `public/` los regeneran los hooks `pre_build` de `Trunk.toml`.
- **`Trunk.toml` multiplataforma.** Los hooks invocan `npx` a secas; el shim
  `.cmd` explícito rompía el build en toda plataforma salvo Windows.

---

## Estado Actual (v0.1.0)

| Capacidad | Estado | Detalle |
|---|---|---|
| Stack | ✅ | Tauri 2 + Leptos 0.8 CSR + Tailwind CSS 3.4 |
| Build | ✅ | Trunk (WASM) + `cargo tauri dev` (desktop) |
| Núcleo compartido | ✅ | Crate `codedocs-core` (tipos, markdown, guardián de rutas, árbol) |
| File Tree | ✅ | Sidebar recursivo, solo `.md`, carpetas primero, recorrido acotado |
| Abrir/Guardar/Crear/Eliminar/Renombrar | ✅ | Commands Tauri, todos confinados al `Workspace` abierto |
| Escritura atómica | ✅ | Temporal + `rename`; tope de 5 MB por documento |
| Editor | ✅ | CodeMirror 6 con `lang-markdown` y resaltado por lenguaje (ya no `<textarea>`) |
| Modos de vista | ✅ | Raw `Ctrl+1` · Format `Ctrl+2` · **Split `Ctrl+3`** · toggle `Ctrl+0` |
| Preview | ✅ | `pulldown-cmark`: tablas GFM, notas al pie, tachado, tasklists, smart punctuation, atributos de encabezado |
| Render backend | ✅ | Uno solo: `codedocs_core::render_markdown`, compartido por frontend y backend |
| Math (KaTeX) | ✅ | `$…$` y `$$…$$`, con carga diferida |
| Mermaid | ✅ | Bloques ```` ```mermaid ````, con carga diferida |
| Sanitización del preview | ✅ | HTML crudo descartado, allowlist por construcción, URLs por esquema |
| Outline / TOC | ✅ | Panel "Contenido" en vivo; click → scroll en el preview |
| Indicador de guardado | ✅ | Punto en el header + etiqueta en la barra de estado |
| Auto-guardado | ✅ | Rebote de 1,5 s con timer cancelable |
| File watching | ✅ | `notify`: recarga si el archivo abierto cambió afuera, refresca el árbol |
| Atajos de teclado | ✅ | Tabla única: guardar, negrita, cursiva, código, tachado, enlace, foco, vistas, nuevo archivo |
| Conteo de palabras/caracteres | ✅ | Barra de estado |
| Barra de estado | ✅ | Selector de vista, conteo, indicador de guardado, botón guardar/limpiar |
| Modo claro/oscuro | ✅ | Toggle con doble clic en el logo |
| Modals | ✅ | Shell único: Delete, Rename, Alert (con Escape) |
| Resize panels | ✅ | Sidebar redimensionable arrastrando el divisor |
| Mock mode (web) | ✅ | Datos demo cuando `!is_tauri()` |
| Toolbar de plantillas | ❌ | **Eliminada** — ya no existe en el código (API Doc, Nota Rápida, Checklist, Generar Índice) |
| Editor WYSIWYG inline | ❌ | Sin edición inline renderizada; el preview es un pane aparte |
| Exportación | ❌ | No implementada |
| Búsqueda | ❌ | No implementada |
| Multi-tab | ❌ | Un solo archivo abierto |
| Config/Preferencias | ❌ | No hay settings |
| CSP | ❌ | `tauri.conf.json` sigue con `"csp": null` |

---

## Arquitectura Objetivo

> Este bloque es **aspiracional**: es el árbol al que se quiere llegar, no el
> estado actual. El árbol real —con `codedocs-core/`, `src/state.rs`,
> `src/actions.rs`, `src/shortcuts.rs` y `src/watch.rs`— está en el
> [README](../README.md#estructura). Las diferencias grandes: no hay `hooks/`
> ni `stores/` (el estado vive en `src/state.rs` y las operaciones en
> `src/actions.rs`), `utils/md.rs` ya no existe, y los módulos de export, search
> e image todavía no se escribieron.

```
codedocs/
├── src/                          # Frontend Leptos (WASM)
│   ├── main.rs
│   ├── app.rs                    # Root component + router
│   ├── components/
│   │   ├── mod.rs
│   │   ├── editor/               # Seamless WYSIWYG editor
│   │   │   ├── mod.rs
│   │   │   ├── block.rs          # Block-level rendering (headings, lists, tables, code)
│   │   │   ├── inline.rs         # Inline formatting (bold, italic, code, links)
│   │   │   ├── math.rs           # KaTeX integration
│   │   │   ├── mermaid.rs        # Mermaid diagram rendering
│   │   │   ├── image.rs          # Image preview + drag & drop
│   │   │   ├── table_editor.rs   # Inline table editing UI
│   │   │   └── status_bar.rs     # Word count, cursor position, encoding
│   │   ├── sidebar/
│   │   │   ├── mod.rs
│   │   │   ├── file_tree.rs      # Refactor del FileTree actual
│   │   │   ├── outline.rs        # Outline panel (TOC navegable)
│   │   │   └── search.rs         # Global search panel
│   │   ├── toolbar/
│   │   │   ├── mod.rs
│   │   │   ├── formatting.rs     # Bold, italic, heading, list buttons
│   │   │   ├── insert.rs         # Insert table, image, code block, math
│   │   │   └── view_modes.rs     # Focus, Typewriter, Source mode toggles
│   │   ├── command_palette.rs    # Ctrl+P command palette
│   │   ├── modals/
│   │   │   ├── mod.rs
│   │   │   ├── preferences.rs    # Settings window
│   │   │   ├── export.rs         # Export dialog
│   │   │   └── about.rs
│   │   └── layout.rs             # Main layout shell
│   ├── hooks/
│   │   ├── mod.rs
│   │   ├── keyboard.rs           # Global keyboard shortcuts
│   │   ├── file_watcher.rs       # External file change detection
│   │   └── drag_drop.rs          # Image/file drag & drop
│   ├── stores/
│   │   ├── mod.rs
│   │   ├── editor_store.rs       # Editor state (content, cursor, mode)
│   │   ├── file_store.rs         # Open files, current file, dirty state
│   │   └── settings_store.rs     # User preferences, theme, keybindings
│   └── utils/
│       ├── mod.rs
│       ├── env.rs
│       ├── markdown.rs           # Frontend markdown parsing/rendering
│       └── tauri_bridge.rs       # Centralized Tauri invoke wrapper
├── src-tauri/                    # Rust backend (Tauri)
│   ├── src/
│   │   ├── main.rs
│   │   ├── lib.rs
│   │   ├── commands/
│   │   │   ├── mod.rs
│   │   │   ├── fs.rs             # File CRUD operations
│   │   │   ├── export.rs         # PDF, HTML, DOCX export
│   │   │   ├── search.rs         # Ripgrep-based global search
│   │   │   └── image.rs          # Image save/optimize
│   │   ├── state.rs              # App state management
│   │   └── utils/
│   │       ├── mod.rs
│   │       ├── md.rs             # Markdown utilities (TOC, etc.)
│   │       └── export.rs         # Export helpers
│   └── Cargo.toml
├── themes/                       # Custom CSS themes
│   ├── github-light.css
│   ├── github-dark.css
│   ├── dracula.css
│   └── solarized.css
├── docs/
│   ├── ROADMAP.md                # This file
│   └── ARCHITECTURE.md           # Technical decisions
├── Cargo.toml
├── Trunk.toml
└── tauri.conf.json
```

---

## Decision Técnica Clave: Motor del Editor

### Opción A: CodeMirror 6 (RECOMENDADA para Fase 1)

**Por qué:**
- Editor maduro, probado en producción (Obsidian usa una variante)
- Soporte nativo de Markdown con syntax highlighting
- Extensible via plugins (math, mermaid, tables)
- Tiene bindings WASM — funciona en Tauri
- Modo "preview inline" implementable como plugin custom
- Undo/redo robusto de fábrica
- Virtual scrolling para archivos grandes

**Cómo encaja en Leptos:**
- Montar CM6 en un `<div>` via `NodeRef`
- CM6 emite eventos → Leptos signals se actualizan
- Leptos renderiza sidebar/outline/status bar reactivo a CM6 state

**Riesgo:** Integración JS↔Leptos requiere bridge manual, pero es factible.

### Opción B: ProseMirror (IDEAL pero más complejo)

**Por qué es el "Santo Grial":**
- Modelo de documento que ES el rendered output (no hay separación source/preview)
- Usado por Notion, Athens Research, Zettlr
- Transformaciones atómicas → undo/redo perfecto
- Tablas nativas via `prosemirror-tables`
- Math via `prosemirror-math`
- Mermaid como nodo custom

**Por qué NO empezar con esto:**
- Curva de aprendizaje brutal (documentación fragmentada)
- Escribir Markdown↔ProseMirror parser bidireccional es trabajo enorme
- En Rust/WASM hay que manejar toda la serialización JS↔Rust

### Opción C: ContentEditable custom (NO recomendado)

- Parece fácil, es una trampa
- `contenteditable` es inconsistente entre browsers
- Selection API es un infierno
- Solo viable si el scope es mínimo

### Estrategia: CodeMirror 6 primero, evaluar ProseMirror en Fase 2

CodeMirror 6 permite iterar rápido con un editor funcional. En Fase 2, si el WYSIWYG seamless es prioritario, se evalúa migrar a ProseMirror o implementar "source mode + preview mode" con transición suave (como Typora: muestra Markdown solo en la línea del cursor).

---

## Fases de Desarrollo

---

### FASE 0: Preparación (3-5 días)

**Objetivo:** Estabilizar fundaciones, instalar herramientas, refactorizar código existente.

#### 0.1 — Limpieza y reorganización

- [x] Refactorizar `layout.rs` (547 líneas) → ahora ~208 líneas y es solo cableado
  - Estado de la app → `src/state.rs` (`EditorState`, `ViewMode`, `SaveState`)
  - Operaciones de archivos → `src/actions.rs` (abrir, guardar, crear, borrar, renombrar, auto-guardado)
  - Atajos → `src/shortcuts.rs`
  - Vigilante de archivos → `src/watch.rs`
  - Sidebar → `components/sidebar/`, editor → `components/editor/`, barra de estado → `components/status_bar.rs`
- [x] Centralizar `invoke()` calls en `utils/tauri_bridge.rs` (se repite en 4 archivos)
  - Ahora hay un wrapper tipado por comando, con el nombre de cada argumento en un solo lugar
- [x] Eliminar `render_markdown` command duplicado (backend usa el mismo parser que el frontend)
  - El command `render_markdown` ya no existe en `src-tauri/src/lib.rs`; ambos lados llaman a `codedocs_core::render_markdown`
- [x] Extraer `FileEntry` a un tipo compartido
  - Concreto: vive en el crate nuevo `codedocs-core` (`types.rs`), reexportado por `src/types.rs` para el frontend

#### 0.2 — Dependencias nuevas

- [ ] `leptos_router` — para future multi-tab / navegación
- [ ] `leptos_meta` — meta tags, title dinámico
- [ ] `tauri-plugin-fs` — file watching, mejor I/O
  - Nota: el file watching se resolvió con el crate `notify` directamente, así que `tauri-plugin-fs` ya no hace falta para eso. Sigue sin estar.
- [ ] `tauri-plugin-notification` — feedback al usuario
- [ ] `tauri-plugin-process` — manejo de app lifecycle
- [ ] `tauri-plugin-clipboard-manager` — clipboard avanzado
- [x] CodeMirror 6 via npm/wasm: `@codemirror/lang-markdown`, `@codemirror/theme-one-dark`

#### 0.3 — Build system

- [ ] Verificar que `cargo tauri dev` funciona sin errores
  - No verificable en el entorno de desarrollo actual: faltan `webkit2gtk-4.1` / `javascriptcoregtk-4.1`. El job `tauri` de CI compila el backend (`cargo check -p codedocs`) en Linux con esas dependencias instaladas, así que la primera señal real de que arranca viene de ahí.
- [x] Agregar un comando de inicio
  - Resuelto con scripts de npm en vez de `dev.ps1` / `dev.sh`, que además de ser multiplataforma sirven para el frontend y para la app: `npm run dev` (trunk serve) y `npm run tauri` (cargo tauri dev)
- [x] Configurar `tailwind.config.js` con theme tokens (colores, fuentes, spacing)
  - Tokens `base.*` y `brand.*` + la fuente `unifraktur`. La escala de `spacing` sigue siendo la de Tailwind por defecto, así que esta casilla es parcial.
- [ ] Crear `themes/` directory con al menos 2 temas base

#### 0.4 — CI básico

- [x] `cargo clippy` en frontend y backend
  - En CI: clippy sobre `codedocs-core` (lado frontend, `-D warnings`) en las tres plataformas, y clippy sobre `codedocs` (backend) en el job `tauri` de Linux. El crate del frontend completo (`codedocs-ui`, target wasm32) se verifica con `cargo check`, no con clippy.
- [x] `cargo fmt --check`
  - En CI, en las tres plataformas
- [x] Build check en CI (GitHub Actions)
  - `.github/workflows/ci.yml`: matriz Linux/macOS/Windows para el frontend (`cargo check --target wasm32`, fmt, clippy y tests de `codedocs-core`) más un job de Linux que instala las dependencias de sistema de Tauri para poder compilar el backend

**Entregable:** Proyecto limpio, compila sin warnings, `cargo tauri dev` estable.

---

### FASE 1: MVP — Editor Funcional (2-3 semanas)

**Objetivo:** CodeDocs es usable para escribir Markdown como en Typora (modo source + preview).

#### 1.1 — Integrar CodeMirror 6 como editor

- [x] Crear componente `CodeMirrorEditor` en Leptos
  - Montar CM6 en `<div>` via `NodeRef<HtmlElement>`
  - Configurar `@codemirror/lang-markdown`
  - Sincronizar contenido CM6 ↔ Leptos signal
  - Tema: `one-dark` para dark mode, `default` para light
- [x] Reemplazar `<textarea>` actual con CM6
- [x] Mantener preview panel como opción (Source mode)

#### 1.2 — Preview mejorado

- [ ] Agregar extensiones de `pulldown-cmark`:
  - `ENABLE_YAML_FRONTMATTER` (metadata block) — sin usar
  - Math: **no** viene de una extensión de `pulldown-cmark`. Se detecta sobre los eventos de texto ya parseados (`split_math`) y se emite como `<span class="math-inline">` / `<span class="math-display">` para que KaTeX lo renderice después. Funciona; queda fuera de esta casilla por diseño.
- [x] Integrar KaTeX rendering en preview (cargar katex.min.js + katex.css)
  - Carga diferida vía `js/preview-bridge.mjs`: los assets no están en `index.html`, se inyectan la primera vez que aparece un placeholder de math
- [x] Integrar Mermaid rendering en preview (cargar mermaid.min.js)
  - Igual de diferido; `securityLevel: "strict"` y `startOnLoad: false` porque el render lo dirige el bridge
- [ ] Sincronizar scroll editor ↔ preview (bidireccional)

#### 1.3 — Outline panel

- [x] Extraer headings del documento (h1-h6) en tiempo real
- [x] Componente `OutlinePanel` en sidebar
- [x] Click en heading → scroll a posición en el preview
  - Antes eran botones sin handler (UI muerta). Ahora llaman `scroll_into_view` sobre el `id` que emitió el renderer; el slug se comparte, no se reimplementa.
- [ ] Highlight heading actual según posición del cursor
- [x] Tabs en sidebar: "Archivos" | "Contenido"

#### 1.4 — Atajos de teclado esenciales

- [x] `Ctrl+S` — Guardar archivo
- [x] `Ctrl+B` — **Bold**
- [x] `Ctrl+I` — *Italic*
- [x] `Ctrl+K` — [Link]
- [x] `Ctrl+E` — `Código inline`
  - La Roadmap decía `Ctrl+Shift+K`, que nunca se asignó. La combinación real es `Ctrl+E` (`EditorCommand::InlineCode`). `Ctrl+Shift+S` es ~~tachado~~.
- [ ] `Ctrl+/` — Toggle heading
- [ ] `Ctrl+Shift+M` — Math block
- [x] `Ctrl+N` — Nuevo archivo
- [ ] `Ctrl+O` — Abrir carpeta
  - Hoy solo hay botón en el sidebar; abrir carpeta usa el diálogo nativo y no tiene atajo.
- [ ] `Ctrl+W` — Cerrar archivo
- [x] `Ctrl+Z` / `Ctrl+Y` — Undo/Redo
  - Viene de fábrica con `basicSetup` de CodeMirror 6, que incluye `history()` y su keymap. No verificado a mano.
- [x] Hook global que intercepte y delegue a CM6 o Tauri
  - `src/shortcuts.rs` recorre una tabla de `EditorCommand` y llama al mismo `run()` desde el keymap de CM6, desde los botones y desde la barra de estado. Antes solo `Ctrl+1`/`Ctrl+2` estaban cableados en el layout y el resto no hacía nada.

#### 1.5 — Modos de vista

- [x] **Source Mode**: Editor CM6 a la izquierda, preview a la derecha (split pane actual)
- [ ] **Live Preview Mode**: Editor CM6 solo, con preview inline para elementos bloque (como VS Code)
  - **No implementado.** No hay `ViewPlugin` ni `Decoration` de CodeMirror. Lo que existe hoy es el pane Split.
- [x] **Reader Mode**: Solo preview, sin editor
  - Se llama "Format" en la UI (`Ctrl+2`)
- [x] Toggle entre modos con botón o atajo
  - Selector segmentado en la barra de estado + `Ctrl+0`/`Ctrl+1`/`Ctrl+2`/`Ctrl+3`

#### 1.6 — Auto-save

- [x] Debounced auto-save (1.5s sin tipeo → guardar)
  - Rebote de 1,5 s con timer cancelable; una ráfaga de pulsaciones produce una sola escritura del texto final
- [x] Indicador visual: "Saved" / "Saving..." / "Unsaved"
  - `SaveState` con `Sin guardar` / `Guardando…` / `Guardado` / `Error al guardar`. Punto de color en el header (rojo si la escritura falló) + etiqueta en la barra de estado.
- [x] Solo en modo Tauri (no web demo)

#### 1.7 — File watching

- [x] Tauri command: `watch_folder(folder_path)` usando `notify` crate
- [x] Evento → frontend
  - **Corregido:** no son tres eventos (`file_changed` / `file_deleted` / `file_created`) sino uno solo, `fs-change`, con un `Vec<String>` de rutas Markdown. Las escrituras propias se filtran en el backend con `WriteLog` (ventana de 1,5 s), que es lo que corta el loop guardado → vigilante → recarga → guardado.
- [x] Si el archivo abierto cambió externamente: recargar
  - Recarga automática, sin prompt, y **solo si no hay cambios sin guardar**: el buffer del usuario gana sobre la copia en disco.
- [x] Refrescar file tree cuando detecte cambios

**Entregable:** CodeDocs abre carpetas, edita Markdown con syntax highlighting, preview con KaTeX+Mermaid, outline navegable, atajos principales, auto-save, file watching.

---

### FASE 2: Editor WYSIWYG Seamless (3-5 semanas)

**Objetivo:** Edición inline como Typora — la sintaxis Markdown se muestra solo en la línea del cursor.

#### 2.1 — Seamless editing (el feature DEFINITORIO)

**Enfoque progresivo (sin ProseMirror):**

- [ ] Implementar "line-level source/preview toggle" en CM6
  - Línea sin cursor → renderizada como HTML (heading grande, lista estilizada, etc.)
  - Línea con cursor → muestra Markdown source
  - Usar CM6 `ViewPlugin` + `Decoration` para swap visual
- [ ] Esto es complejo pero factible — CM6 es lo suficientemente extensible
- [ ] Referencia: [codemirror-block-editing](https://github.com/nicktomlin/codemirror-block-editing)

**Alternativa si CM6 no alcanza:**

- [ ] Evaluar ProseMirror como reemplazo
- [ ] Crear Markdown → ProseMirror parser
- [ ] Crear ProseMirror → Markdown serializer
- [ ] Implementar nodos custom para math, mermaid, callouts

#### 2.2 — Tablas inline editables

- [ ] Modo source: editar Markdown table syntax
- [ ] Modo preview/WYSIWYG: tabla HTML editable
  - Click en celda → contentEditable
  - Añadir fila/columna con botones `+`
  - Delete fila/columna con click derecho
  - Resize columnas arrastrando bordes
- [ ] Sincronizar cambios → Markdown source

#### 2.3 — Modo Focus y Typewriter

- [ ] **Focus Mode**: Opacity baja en líneas que no son del párrafo actual
- [ ] **Typewriter Mode**: Cursor siempre centrado verticalmente (scroll constante)
- [ ] Toggles en toolbar o atajos (`Ctrl+Shift+F`, `Ctrl+Shift+T`)

#### 2.4 — Command Palette

- [ ] `Ctrl+P` → palette con:
  - Archivos recientes (fuzzy search)
  - Comandos (heading, bold, insert table, etc.)
  - Snippets
- [ ] Componente `CommandPalette` con búsqueda fuzzy
- [ ] Integrar con file store + editor commands

#### 2.5 — Búsqueda

- [ ] **En archivo**: `Ctrl+F` → search bar con highlight de resultados (CM6 search addon)
- [ ] **Global**: `Ctrl+Shift+F` → panel en sidebar
  - Tauri command: `search_in_project(folder, query)` usando `grep` o `ignore` crate
  - Resultados con preview de línea + click para abrir

#### 2.6 — Gestión de imágenes

- [ ] Drag & drop de imágenes al editor
  - Tauri command: `save_image(folder, filename, data)` → copia a `assets/` relativo
  - Inserta `![alt](./assets/image.png)` en el editor
- [ ] Paste de imágenes del clipboard (Ctrl+V con imagen)
- [ ] Preview de imágenes inline (con tamaño configurable)
- [ ] Click en imagen → abrir en visor externo o modal

#### 2.7 — YAML Front Matter

- [ ] Parsear `---` blocks al inicio del documento
- [ ] Renderizar como tabla o formulario inline
- [ ] Exponer metadata al editor (title, date, tags)

#### 2.8 — Callouts / GitHub Alerts nativos

- [ ] Reemplazar el hack de `content.replace("[!NOTE]", "**NOTE**")`
  - Ese hack ya no está en el código: `grep` por `[!NOTE]` / `callout` sobre `src/`, `js/` y `codedocs-core/` no devuelve nada. La casilla queda abierta solo por el render nativo de GitHub Alerts.
- [ ] Parser custom en pulldown-cmark o post-procesador HTML
- [ ] Renderizar con iconos y colores por tipo (NOTE, TIP, WARNING, etc.)

**Entregable:** CodeDocs tiene edición WYSIWYG seamless, tablas editables, focus/typewriter, command palette, búsqueda global, drag & drop de imágenes, front matter, callouts.

---

### FASE 3: Polish y Exportación (2-4 semanas)

**Objetivo:** CodeDocs es un producto pulido, exportable, temable.

#### 3.1 — Sistema de temas

- [ ] CSS variables para todos los tokens de color, tipografía, spacing
- [ ] Cargar temas desde `themes/*.css`
- [ ] Preferences: seleccionar tema de la lista
- [ ] Temas incluidos:
  - GitHub Light / Dark
  - Dracula
  - Solarized Light / Dark
  - One Dark (editor)
  - Newsprint (print-like)
- [ ] Custom CSS por documento (leer `theme:` del front matter)
- [ ] Auto-detectar OS dark mode preference

#### 3.2 — Exportación

- [ ] **HTML**: pulldown-cmark → HTML completo con estilos inline
- [ ] **PDF**: via Tauri webview print-to-PDF o `wkhtmltopdf`
  - `tauri::webview::print()` o usar headless browser
- [ ] **DOCX**: via `pandoc` si está instalado, o `docx-rs` crate
- [ ] **Markdown original**: save as (con front matter preservado)
- [ ] Export dialog con opciones (format, template, include styles)

#### 3.3 — Multi-tab / Multi-ventana

- [ ] Tabs en el header del editor (como VS Code)
- [ ] State por tab: content, cursor position, scroll, dirty flag
- [ ] `Ctrl+Tab` → switch entre tabs
- [ ] Cerrar tab con `X` o click medio
- [ ] Preview de tab con hover (tooltip con primeras líneas)
- [ ] Evaluar multi-ventana nativa Tauri ( opcional)

#### 3.4 — Preferencias / Settings

- [ ] Ventana de preferencias (modal o panel)
  - Editor: font family, font size, tab size, word wrap, line numbers
  - Theme: seleccionar de lista
  - Save: auto-save on/off, interval
  - Export: default format, include styles
  - Keybindings: ver/editar atajos
- [ ] Persistir en `~/.config/codedocs/settings.json` via Tauri fs
- [ ] Aplicar cambios en tiempo real (signals reactivo)

#### 3.5 — Status bar

- [x] Línea de estado en la parte inferior
  - [x] Palabras / Caracteres
  - [ ] Líneas
  - [ ] Posición del cursor (Línea:Col)
  - [ ] Encoding (UTF-8)
  - [ ] Tipo de archivo (Markdown)
  - [x] Modo de vista actual — selector segmentado Raw / Split / Format, que además es el toggle
  - [x] Indicador de guardado — `Sin guardar` / `Guardando…` / `Guardado` / `Error al guardar`
  - También tiene los botones Guardar (`Ctrl+S`) y Limpiar editor, que no estaban en el plan original.

#### 3.6 — Undo/Redo fino

- [ ] CM6 ya tiene undo/redo robusto (verificar que funciona correctamente)
- [ ] Agregar "Undo stack" visual (opcional, tipo VS Code timeline)
- [ ] `Ctrl+Shift+Z` para redo

#### 3.7 — Context menu (click derecho)

- [ ] Menú contextual rico en el editor:
  - Cut / Copy / Paste
  - Heading → submenu (H1-H6)
  - Bold, Italic, Strikethrough
  - Insert Link, Image, Code Block, Math Block, Table
  - Copy as HTML
  - Formatear como código
- [ ] Menú contextual en file tree:
  - New File, New Folder
  - Rename, Delete, Duplicate
  - Copy Path, Reveal in Explorer

**Entregable:** CodeDocs tiene temas, exportación, multi-tab, preferencias, status bar, context menus.

---

### FASE 4: Diferenciación y Distribución (2-4 semanas)

**Objetivo:** CodeDocs es distribuíble, extensible, con features que lo distinguen.

#### 4.1 — Mermaid avanzado

- [ ] Soporte completo de diagramas Mermaid (flowchart, sequence, class, state, ER, gantt, pie, mindmap)
- [ ] Live preview interactivo (zoom, pan)
- [ ] Exportar diagrama como PNG/SVG
- [ ] Editor de Mermaid con syntax highlighting

#### 4.2 — Wiki-links / Enlaces relativos

- [ ] `[[otro-archivo]]` syntax → click para navegar
- [ ] Resolver ruta relativa al archivo actual
- [ ] Backlinks: mostrar qué archivos enlazan al actual
- [ ] Crear archivo automáticamente si no existe al hacer click

#### 4.3 — Spellcheck

- [ ] Integrar `nuspell` o `hunspell` via Tauri command
- [ ] Subrayado de errores en el editor (CM6 decorations)
- [ ] Sugerencias en context menu
- [ ] Soporte multi-idioma (es, en)

#### 4.4 — Word count / Reading time

- [ ] Contador en status bar (palabras, caracteres, párrafos)
- [ ] Estimación de tiempo de lectura (200 wpm)
- [ ] Objetivo de palabras (opcional, meta configurable)

#### 4.5 — Plugins / Extensions

- [ ] Sistema simple de plugins:
  - Archivo `codedocs-plugin.json` en carpeta del usuario
  - Define: commands, keybindings, snippets
  - JS scripts cargados via Tauri eval
- [ ] Ejemplos: snippet manager, linter, custom export

#### 4.6 — Git integration (básico)

- [ ] Detectar si la carpeta es un repo git
- [ ] `git status` → indicador en status bar
- [ ] `git diff` → view changes
- [ ] `git commit` → dialog con mensaje
- [ ] NO es un Git GUI — solo lo esencial para documentación

#### 4.7 — Build cross-platform

- [ ] Windows: `.msi` y `.exe` (NSIS)
- [ ] macOS: `.dmg`
- [ ] Linux: `.AppImage` y `.deb`
- [ ] GitHub Actions: build matrix para 3 plataformas
- [ ] Auto-update via `tauri-plugin-updater`

#### 4.8 — Documentación y distribución

- [x] README con instrucciones de build
- [x] CONTRIBUTING.md
- [x] LICENSE (MIT)
- [ ] GitHub Releases con binaries
- [ ] Website/landing page (opcional)

**Entregable:** CodeDocs es distribuíble en 3 plataformas, con Mermaid avanzado, wiki-links, spellcheck, plugins, git básico.

---

## Priorización Visual

```
                    Impacto en "Typora-like"
                    ^
                    |
   F2: Seamless    |  ★★★★★  (DEFINITORIO)
   F2: Tables      |  ★★★★☆
   F1: CM6 Editor  |  ★★★★☆
   F1: Outline     |  ★★★☆☆
   F1: Shortcuts   |  ★★★☆☆
   F3: Themes      |  ★★★☆☆
   F3: Export      |  ★★★☆☆
   F2: Cmd Palette |  ★★☆☆☆
   F2: Search      |  ★★☆☆☆
   F4: Wiki-links  |  ★★☆☆☆
   F4: Plugins     |  ★☆☆☆☆
                    |
                    +-------------------------> Esfuerzo
                    Bajo          Medio         Alto
```

---

## Riesgos y Mitigaciones

| Riesgo | Probabilidad | Impacto | Mitigación |
|---|---|---|---|
| CM6 seamless editing muy complejo | Alta | Alto | Fallback a source+preview mode pulido; evaluar ProseMirror |
| KaTeX/Mermaid en WASM pesado | Media | Medio | Lazy loading; solo renderizar visible; web workers |
| File watching cross-platform issues | Media | Bajo | `notify` crate maneja diferencias; tests en Win/Mac/Linux |
| Performance con archivos grandes (>1MB) | Media | Alto | Virtual scrolling CM6; debounced parsing; Tauri commands para parseo pesado |
| Integración JS↔Leptos frágil | Media | Medio | Wrapper tipado en `tauri_bridge.rs`; tests de integración |
| ProseMirror migration disruptiva | Baja | Alto | Abstracción de editor interface; swap limpio si se migra |

---

## Métricas de Éxito por Fase

| Fase | Criterio de aceptación |
|---|---|
| F0 | `cargo tauri dev` funciona sin errores; clippy limpio |
| F1 | Puedo escribir un documento completo con headings, listas, code blocks, math, mermaid; outline navega; atajos principales funcionan; auto-save funciona |
| F2 | Edición seamless (sin split pane); tablas editables inline; command palette funcional; búsqueda global funciona; drag & drop de imágenes |
| F3 | 4+ temas disponibles; export a PDF y HTML; tabs funcionan; preferencias se persisten |
| F4 | Builds para Win/Mac/Linux; wiki-links navegan; spellcheck subraya errores |

---

## Comandos de Desarrollo

```bash
# Desarrollo (desktop)
cargo tauri dev            # o: npm run tauri

# Desarrollo (solo frontend en navegador)
trunk serve                # o: npm run dev

# Build producción
cargo tauri build

# Lint
cargo clippy -p codedocs-core --all-targets -- -D warnings
cargo clippy -p codedocs --all-targets          # backend; necesita webkit2gtk-4.1
cargo fmt --all -- --check

# Compilación
cargo check --target wasm32-unknown-unknown      # frontend
cargo check -p codedocs                          # backend; necesita webkit2gtk-4.1

# Test
cargo test -p codedocs-core
```

`cargo test --workspace` **no** funciona: el paquete del frontend es un binario
para `wasm32` y `src-tauri` no compila sin `webkit2gtk-4.1`. La suite que se puede
correr en el host es la de `codedocs-core`.

---

## Stack Tecnológico Final

Estado real de las dependencias, contrasted con `Cargo.toml`, `package.json` y
`src-tauri/Cargo.toml`. Lo que todavía no está se marca como pendiente.

| Capa | Tecnología | Versión |
|---|---|---|
| Desktop | Tauri | 2.x |
| Frontend | Leptos | 0.8 (CSR) |
| Núcleo compartido | `codedocs-core` (Rust, sin Tauri ni Leptos) | 0.1.0 |
| Editor | CodeMirror 6 | latest (npm, bundle con esbuild) |
| Markdown | pulldown-cmark | 0.13 |
| Math | KaTeX | 0.17 (vendorizado en `public/vendor/katex/`, carga diferida) |
| Diagrams | Mermaid | 11.x (vendorizado en `public/vendor/mermaid/`, carga diferida) |
| CSS | Tailwind CSS | 3.4 |
| File I/O | crate `notify` + `std::fs` sobre `codedocs-core` | 7.x |
| Dialogs | tauri-plugin-dialog | 2.x |
| Apertura de enlaces | tauri-plugin-opener | 2.x |
| Build | Trunk | 0.x |
| Búsqueda global | crate `ignore` (núcleo de ripgrep) | pendiente |
| Export PDF | Tauri print / weasyprint | TBD |
| Export DOCX | pandoc / docx-rs | TBD |
