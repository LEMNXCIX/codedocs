# CodeDocs — Roadmap a Typora

## Estado Actual (v0.1.0)

| Capacidad | Estado | Detalle |
|---|---|---|
| Stack | ✅ | Tauri 2 + Leptos 0.8 CSR + Tailwind CSS |
| Build | ✅ | Trunk (WASM) + `cargo tauri dev` (desktop) |
| File Tree | ✅ | Sidebar recursivo, solo `.md`, dirs primero |
| Abrir/Guardar/Crear/Eliminar/Renombrar | ✅ | Commands Tauri funcionales |
| Editor | ⚠️ | `<textarea>` plano con split pane editor/preview |
| Preview | ✅ | `pulldown-cmark` con GFM tables, footnotes, strikethrough, tasklists |
| Render backend | ✅ | Duplicado: `pulldown-cmark` en frontend (WASM) Y backend (Rust) |
| Modo claro/oscuro | ✅ | Toggle via dblclick en logo |
| Toolbar de plantillas | ✅ | API Doc, Nota Rápida, Checklist, Generar Índice |
| Modals | ✅ | Delete, Rename, Alert (con Escape) |
| Resize panels | ✅ | Sidebar + editor ratio arrastrable |
| Mock mode (web) | ✅ | Datos demo cuando `!is_tauri()` |
| Outline / TOC | ❌ | Solo generación de texto, no panel navegable |
| Editor WYSIWYG | ❌ | Sin edición inline renderizada |
| Atajos de teclado | ❌ | Solo los nativos del textarea |
| Math (KaTeX) | ❌ | No renderizado |
| Mermaid | ❌ | No renderizado |
| Exportación | ❌ | No implementada |
| Búsqueda | ❌ | No implementada |
| Multi-tab | ❌ | Un solo archivo abierto |
| File watching | ❌ | No detecta cambios externos |
| Config/Preferencias | ❌ | No hay settings |

---

## Arquitectura Objetivo

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

- [ ] Refactorizar `layout.rs` (547 líneas) → extraer componentes
  - Sidebar → `components/sidebar/`
  - Header bar → `components/layout.rs` (solo shell)
  - Editor pane → `components/editor/`
- [ ] Centralizar `invoke()` calls en `utils/tauri_bridge.rs` (se repite en 4 archivos)
- [ ] Eliminar `render_markdown` command duplicado (backend usa el mismo parser que el frontend)
- [ ] Extraer `FileEntry` a un tipo compartido o al menos un archivo dedicado

#### 0.2 — Dependencias nuevas

- [ ] `leptos_router` — para future multi-tab / navegación
- [ ] `leptos_meta` — meta tags, title dinámico
- [ ] `tauri-plugin-fs` — file watching, mejor I/O
- [ ] `tauri-plugin-notification` — feedback al usuario
- [ ] `tauri-plugin-process` — manejo de app lifecycle
- [ ] `tauri-plugin-clipboard-manager` — clipboard avanzado
- [ ] CodeMirror 6 via npm/wasm: `@codemirror/lang-markdown`, `@codemirror/theme-one-dark`

#### 0.3 — Build system

- [ ] Verificar que `cargo tauri dev` funciona sin errores
- [ ] Agregar script `dev.ps1` / `dev.sh` para un comando de inicio
- [ ] Configurar `tailwind.config.js` con theme tokens (colores, fuentes, spacing)
- [ ] Crear `themes/` directory con al menos 2 temas base

#### 0.4 — CI básico

- [ ] `cargo clippy` en frontend y backend
- [ ] `cargo fmt --check`
- [ ] Build check en CI (GitHub Actions)

**Entregable:** Proyecto limpio, compila sin warnings, `cargo tauri dev` estable.

---

### FASE 1: MVP — Editor Funcional (2-3 semanas)

**Objetivo:** CodeDocs es usable para escribir Markdown como en Typora (modo source + preview).

#### 1.1 — Integrar CodeMirror 6 como editor

- [ ] Crear componente `CodeMirrorEditor` en Leptos
  - Montar CM6 en `<div>` via `NodeRef<HtmlElement>`
  - Configurar `@codemirror/lang-markdown`
  - Sincronizar contenido CM6 ↔ Leptos signal
  - Tema: `one-dark` para dark mode, `default` para light
- [ ] Reemplazar `<textarea>` actual con CM6
- [ ] Mantener preview panel como opción (Source mode)

#### 1.2 — Preview mejorado

- [ ] Agregar extensiones de `pulldown-cmark`:
  - `ENABLE_MATH` si está disponible, o pre-procesar `$...$` y `$$...$$` para KaTeX
  - `ENABLE_YAML_FRONTMATTER` (metadata block)
- [ ] Integrar KaTeX rendering en preview (cargar katex.min.js + katex.css)
- [ ] Integrar Mermaid rendering en preview (cargar mermaid.min.js)
- [ ] Sincronizar scroll editor ↔ preview (bidireccional)

#### 1.3 — Outline panel

- [ ] Extraer headings del documento (h1-h6) en tiempo real
- [ ] Componente `OutlinePanel` en sidebar
- [ ] Click en heading → scroll a posición en editor/preview
- [ ] Highlight heading actual según posición del cursor
- [ ] Tabs en sidebar: "Files" | "Outline"

#### 1.4 — Atajos de teclado esenciales

- [ ] `Ctrl+S` — Guardar archivo
- [ ] `Ctrl+B` — **Bold**
- [ ] `Ctrl+I` — *Italic*
- [ ] `Ctrl+K` — [Link]
- [ ] `Ctrl+Shift+K` — `Code inline`
- [ ] `Ctrl+/` — Toggle heading
- [ ] `Ctrl+Shift+M` — Math block
- [ ] `Ctrl+N` — Nuevo archivo
- [ ] `Ctrl+O` — Abrir carpeta
- [ ] `Ctrl+W` — Cerrar archivo
- [ ] `Ctrl+Z` / `Ctrl+Y` — Undo/Redo (ya en CM6)
- [ ] Hook global `keyboard.rs` que intercepte y delegue a CM6 o Tauri

#### 1.5 — Modos de vista

- [ ] **Source Mode**: Editor CM6 a la izquierda, preview a la derecha (split pane actual)
- [ ] **Live Preview Mode**: Editor CM6 solo, con preview inline para elementos bloque (como VS Code)
- [ ] **Reader Mode**: Solo preview, sin editor
- [ ] Toggle entre modos con botón o atajo

#### 1.6 — Auto-save

- [ ] Debounced auto-save (1.5s sin tipeo → guardar)
- [ ] Indicador visual: "Saved" / "Saving..." / "Unsaved"
- [ ] Solo en modo Tauri (no web demo)

#### 1.7 — File watching

- [ ] Tauri command: `watch_folder(folder_path)` usando `notify` crate
- [ ] Evento → frontend: "file_changed" / "file_deleted" / "file_created"
- [ ] Si el archivo abierto cambió externamente: prompt "Reload?" o auto-reload
- [ ] Refrescar file tree cuando detecte cambios

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

- [ ] Reemplazar el hack actual de `content.replace("[!NOTE]", "**NOTE**")`
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

- [ ] Línea de estado en la parte inferior:
  - Palabras / Caracteres / Líneas
  - Posición del cursor (Línea:Col)
  - Encoding (UTF-8)
  - Tipo de archivo (Markdown)
  - Modo de vista actual
  - Indicador de guardado

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

- [ ] README con instrucciones de build
- [ ] CONTRIBUTING.md
- [ ] LICENSE (MIT recomendado)
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
cargo tauri dev

# Desarrollo (solo frontend en navegador)
trunk serve

# Build producción
cargo tauri build

# Lint
cargo clippy --workspace
cargo fmt --check

# Test
cargo test --workspace
```

---

## Stack Tecnológico Final

| Capa | Tecnología | Versión |
|---|---|---|
| Desktop | Tauri | 2.x |
| Frontend | Leptos | 0.8 (CSR) |
| Editor | CodeMirror 6 | latest (via WASM) |
| Markdown | pulldown-cmark | 0.13 |
| Math | KaTeX | 0.16 (via CDN/local) |
| Diagrams | Mermaid | 11.x (via CDN/local) |
| CSS | Tailwind CSS | 3.4 |
| File I/O | tauri-plugin-fs | 2.x |
| Dialogs | tauri-plugin-dialog | 2.x |
| Search | ignore crate (ripgrep core) | 0.4 |
| Export PDF | Tauri print / weasyprint | TBD |
| Export DOCX | pandoc / docx-rs | TBD |
| Build | Trunk | 0.x |
