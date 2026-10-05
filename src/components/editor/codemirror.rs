use leptos::prelude::*;
use leptos::reactive::spawn_local;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::state::EditorState;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = __codedocs_createEditor)]
    fn cm_create_editor(parent: &web_sys::Element, initial_content: &str, is_dark: bool);

    #[wasm_bindgen(js_name = __codedocs_getContent)]
    fn cm_get_content() -> String;

    #[wasm_bindgen(js_name = __codedocs_setContent)]
    fn cm_set_content(content: &str);

    #[wasm_bindgen(js_name = __codedocs_setTheme)]
    fn cm_set_theme(is_dark: bool);

    #[wasm_bindgen(js_name = __codedocs_setOnChange)]
    fn cm_set_on_change(callback: &js_sys::Function);

    #[wasm_bindgen(js_name = __codedocs_focus)]
    fn cm_focus();

    #[wasm_bindgen(js_name = __codedocs_destroyEditor)]
    fn cm_destroy();

    /// Adds or removes `wrapper` around the selection, depending on whether it
    /// is already wrapped.
    ///
    /// The JS name must match the global exported by `js/codemirror-bridge.mjs`
    /// exactly; the previous declarations used `__codedocs_wrapSelection` and
    /// `__codedocs_insertLink`, while the bridge exported the snake_case
    /// spellings, so these calls silently resolved to `undefined` at runtime.
    #[wasm_bindgen(js_name = __codedocs_toggle_wrap)]
    fn cm_toggle_wrap(wrapper: &str);

    #[wasm_bindgen(js_name = __codedocs_insert_link)]
    fn cm_insert_link();

    /// Returns a promise that settles once every KaTeX/Mermaid pass triggered
    /// by the most recent render has finished.
    #[wasm_bindgen(js_name = __codedocs_render_enhancements_async, catch)]
    async fn cm_render_enhancements_async() -> Result<JsValue, JsValue>;

    /// Registers the span provider the editor calls with the full document
    /// text on every change. The callback never modifies the document; it
    /// only returns decoration data: an `Array` of two elements, a
    /// `Uint32Array` with `ranges` and a `Uint8Array` with the `SpanTag`
    /// discriminants of `tags`.
    ///
    /// The JS name must match the global exported by
    /// `js/codemirror-bridge.mjs` exactly; `check-bridge-contract.mjs`
    /// verifies this contract.
    #[wasm_bindgen(js_name = __codedocs_setSpanProvider)]
    fn cm_set_span_provider(callback: &js_sys::Function);

    /// Shows or hides the raw markdown source in live-format mode.
    ///
    /// Same contract as above; implemented on the JS side by a later task.
    #[wasm_bindgen(js_name = __codedocs_setSourceVisible)]
    fn cm_set_source_visible(is_visible: bool);

    /// Moves the cursor to `offset` (UTF-16 units) and scrolls it into view.
    ///
    /// The JS name must match the global exported by
    /// `js/codemirror-bridge.mjs` exactly; `check-bridge-contract.mjs`
    /// verifies this contract.
    #[wasm_bindgen(js_name = __codedocs_setCursor)]
    fn cm_set_cursor(offset: u32);
}

/// Move the editor cursor to `offset` (UTF-16 units, as carried by
/// [`codedocs_core::markdown::Heading::offset`]) and scroll it into view.
///
/// Called by the outline panel: with no preview pane left, clicking a
/// heading jumps the cursor there instead of scrolling a div.
pub fn set_cursor(offset: u32) {
    cm_set_cursor(offset);
}

/// CodeMirror 6 host component.
///
/// Mounts the editor once into a `NodeRef` div and then keeps the Rust signal
/// and the editor in sync. Three details matter:
///
/// * The change callback, the span-provider callback and the save callback
///   are `Closure`s that must outlive this function, so they are leaked
///   deliberately (`forget`) — the editor itself is destroyed in `on_cleanup`,
///   which is what actually stops them being invoked.
/// * Content is only pushed into the editor when it differs from what the
///   editor last reported. Without that, the editor's own change event would
///   feed back into the signal and re-trigger the sync effect forever.
/// * The JS side keeps a single editor instance, so mounting a second one
///   destroys the first. Only one `CodeMirrorEditor` may therefore be mounted
///   at a time.
#[component]
pub fn CodeMirrorEditor(state: EditorState, on_save: Callback<()>) -> impl IntoView {
    let container_ref = NodeRef::<leptos::html::Div>::new();
    let mounted = RwSignal::new(false);

    // Mount once the container exists.
    Effect::new(move |_| {
        if mounted.get_untracked() {
            return;
        }
        let Some(el) = container_ref.get() else {
            return;
        };

        let set_content = state.content;
        let change_closure = Closure::<dyn Fn(String)>::new(move |new_text: String| {
            set_content.set(new_text);
        });
        cm_set_on_change(change_closure.as_ref().unchecked_ref());
        change_closure.forget();

        // Span provider for live-format mode: the editor calls back with the
        // full document text and gets decoration data. Markdown is the
        // document — this never modifies it, it only labels spans. Leaked
        // deliberately (`forget`), like the other closures: the editor's
        // `on_cleanup` destroy is what stops it being invoked.
        let span_closure = Closure::<dyn FnMut(String) -> JsValue>::new(move |text: String| {
            let live = codedocs_core::live_spans(&text);
            let ranges = js_sys::Uint32Array::from(live.ranges.as_slice());
            let tag_bytes: Vec<u8> = live.tags.iter().map(|t| *t as u8).collect();
            let tags = js_sys::Uint8Array::from(tag_bytes.as_slice());
            let out = js_sys::Array::new();
            out.push(&ranges);
            out.push(&tags);
            out.into()
        });
        cm_set_span_provider(span_closure.as_ref().unchecked_ref());
        span_closure.forget();

        let save_closure = Closure::<dyn Fn()>::new(move || {
            on_save.run(());
        });
        let _ = js_sys::Reflect::set(
            &js_sys::global(),
            &JsValue::from_str("__codedocs_save"),
            save_closure.as_ref(),
        );
        save_closure.forget();

        cm_create_editor(
            &el,
            &state.content.get_untracked(),
            state.is_dark.get_untracked(),
        );
        mounted.set(true);
    });

    // Push external changes (file opened, watcher reload, clear) into the editor.
    //
    // Tracks `is_dark` too, so a theme toggle that rebuilt the editor also
    // re-syncs its content; `cm_get_content` makes the write a no-op when the
    // text already matches.
    Effect::new(move |_| {
        let _ = state.is_dark.get();
        if !mounted.get_untracked() {
            return;
        }
        let content = state.content.get();
        if content != cm_get_content() {
            cm_set_content(&content);
        }
    });

    Effect::new(move |_| {
        if mounted.get_untracked() {
            cm_set_theme(state.is_dark.get());
        }
    });

    // Push the source-visibility toggle into the editor. The toggle lives in
    // Rust (`EditorCommand::ToggleSource` flips `state.source_mode`), so this
    // effect is the single path into JS — nothing else calls
    // `__codedocs_setSourceVisible`.
    Effect::new(move |_| {
        let visible = state.source_mode.get();
        if mounted.get_untracked() {
            cm_set_source_visible(visible);
        }
    });

    on_cleanup(cm_destroy);

    view! {
        <div
            node_ref=container_ref
            class="w-full h-full overflow-hidden codemirror-container"
        />
    }
}

/// Markdown formatting actions.
///
/// A single dispatch table so the keyboard-shortcut install and the toolbar
/// buttons call the same functions. `Nullary` actions take no editor state;
/// they exist so the table can also carry navigation commands.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EditorCommand {
    Save,
    Bold,
    Italic,
    InlineCode,
    Strikethrough,
    Link,
    Focus,
    ToggleSource,
    NewFile,
    ExportPdf,
}

impl EditorCommand {
    /// Run the command.
    pub fn run(self, state: EditorState, on_save: Callback<()>) {
        match self {
            Self::Save => on_save.run(()),
            Self::Bold => cm_toggle_wrap("**"),
            Self::Italic => cm_toggle_wrap("*"),
            Self::InlineCode => cm_toggle_wrap("`"),
            Self::Strikethrough => cm_toggle_wrap("~~"),
            Self::Link => cm_insert_link(),
            Self::Focus => cm_focus(),
            Self::ToggleSource => state.source_mode.update(|b| *b = !*b),
            Self::NewFile => spawn_local(async move {
                // Same rule as the sidebar button: no folder means ask for one,
                // not tell the user they cannot create a file.
                if let Some(folder) = crate::actions::ensure_workspace(state).await {
                    crate::actions::create_file(state, folder);
                }
            }),
            // Bound here and nowhere else. `Mod-p` is deliberately *not* in the
            // CodeMirror keymap below: `Mod-s` is bound in both places, and one
            // Ctrl+S reaches the save path twice — which is why `save_now` needs
            // a re-entrancy guard. A command with a native dialog and a
            // subprocess behind it must not inherit that, and
            // `scripts/check-export-pdf.cjs` counts the invocations to prove it.
            Self::ExportPdf => crate::actions::export_pdf(state),
        }
    }
}

/// The binding for `command`, as `(needs_modifier, key_spec)`.
///
/// `None` is not currently produced, but the signature allows for a command
/// that is deliberately unreachable from the keyboard; callers already treat
/// `None` as "skip" rather than panicking.
pub fn shortcut_for(command: EditorCommand) -> Option<(bool, &'static str)> {
    Some(match command {
        EditorCommand::Save => (true, "s"),
        EditorCommand::Bold => (true, "b"),
        EditorCommand::Italic => (true, "i"),
        EditorCommand::InlineCode => (true, "e"),
        EditorCommand::Strikethrough => (true, "shift+s"),
        EditorCommand::Link => (true, "k"),
        EditorCommand::Focus => (true, "shift+f"),
        EditorCommand::ToggleSource => (true, "/"),
        EditorCommand::NewFile => (true, "n"),
        EditorCommand::ExportPdf => (true, "p"),
    })
}
