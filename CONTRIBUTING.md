# Cómo contribuir

Gracias por querer tocar CodeDocs. Esto es lo mínimo que necesitás.

## Requisitos

- **Rust** estable + el target WASM:
  ```bash
  rustup target add wasm32-unknown-unknown
  ```
- **Trunk**: `cargo install trunk`
- **Node.js** (LTS)
- **Linux**: las dependencias de sistema de Tauri v2 —
  `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libayatana-appindicator3-dev`,
  `librsvg2-dev`, `build-essential`, `curl`, `wget`, `file`, `libssl-dev`,
  `libxdo-dev`. La lista completa está en el [README](README.md#requisitos).
  Sin ellas, `cargo check -p codedocs` (el backend) no compila.

## Puesta en marcha

```bash
npm install
npm run tauri      # app de escritorio
npm run dev        # solo el frontend, en el navegador
```

Si tocás `js/codemirror-bridge.mjs` o `js/preview-bridge.mjs`, corré
`npm run build:cm` / `npm run build:preview` (o compilá una vez): los bundles
generados no están en el control de versiones.

## Antes de abrir un PR

```bash
cargo fmt --all
cargo clippy -p codedocs-core --all-targets -- -D warnings
cargo test  -p codedocs-core
cargo check --target wasm32-unknown-unknown   # frontend
cargo check -p codedocs                        # backend (necesita webkit)
```

CI corre lo mismo en Linux, macOS y Windows, más un job exclusivo de Linux para
el backend Tauri.

## Convenciones

- **Comentarios en español, y explican el porqué.** Un comentario que repite lo
  que hace la línea siguiente no vale el ruido. Cuando algo parece obvio y no lo
  es, ese es el que lleva comentario.
- **`cargo fmt` sin excusas.** El código se formatea solo antes de commitear.
- **Clippy limpio** en todo lo que toque.
- **Tests para los cambios de lógica.** Sobre todo en `codedocs-core`: ahí viven
  el render de Markdown, el guardián de rutas y el recorrido del árbol, y es la
  única suite que se puede correr en el host sin webkit.

## Dónde va cada cosa

- **Lógica de archivos o de Markdown → `codedocs-core/`, con tests.** Es un crate
  Rust pelado, sin Tauri ni Leptos, justamente para poder testearlo en el host y
  compartirlo entre el frontend y el backend. Si tu lógica de archivos no está
  ahí, probablemente debería estar.
- **Estado y operaciones del editor → `src/state.rs` y `src/actions.rs`.**
- **Atajos de teclado → `src/shortcuts.rs`**, y el comando en la tabla de
  `src/components/editor/codemirror.rs`. Todo atajo pasa por ahí.
- **Llamadas al backend → `src/utils/tauri_bridge.rs`.** Un wrapper tipado por
  comando; nunca invoques `invoke()` suelto desde un componente.
- **Comentarios en español también en los strings de cara al usuario** (errores,
  toasts, etiquetas).

## Cosas que conviene saber antes de tocar

- **CSP en `null`.** La defensa del preview es el modelo de allowlist de
  `codedocs-core` (HTML crudo descartado, URLs con esquema validado), no la CSP.
  Si activás la CSP, `js/preview-bridge.mjs` deja de poder inyectar KaTeX y
  Mermaid por `document.createElement` salvo que la allowlist incluya `script-src`.
- **Los escrituras al disco son atómicas** (temporal + `rename`). No vuelvas a
  `fs::write` directo: trunca primero y un crash a mitad de escritura destruye el
  documento.
- **El auto-guardado tiene rebote y un timer cancelable.** Si tocás
  `src/state.rs` o `src/actions.rs`, mantené el `content != last_saved` y el token
  de carga: son los que evitan el loop guardado → vigilante → recarga → guardado.
- **El roadmap dice la verdad.** Si tocás algo que cambia una capacidad de
  `docs/ROADMAP.md`, actualizá la casilla en el mismo PR. Una casilla que miente
  es peor que una casilla vacía.

## Reportes de bugs

Abrí un issue con: qué esperabas, qué pasó, versión (`v0.1.0`), sistema
operativo, y si podés, un documento `.md` mínimo que lo reproduzca.