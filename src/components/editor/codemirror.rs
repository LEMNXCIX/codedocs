use leptos::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::state::{EditorState, ViewMode};

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
}

/// CodeMirror 6 host component.
///
/// Mounts the editor once into a `NodeRef` div and then keeps the Rust signal
/// and the editor in sync. Three details matter:
///
/// * The change callback and the save callback are `Closure`s that must outlive
///   this function, so they are leaked deliberately (`forget`) — the editor
///   itself is destroyed in `on_cleanup`, which is what actually stops them
///   being invoked.
/// * Content is only pushed into the editor when it differs from what the
///   editor last reported. Without that, the editor's own change event would
///   feed back into the signal and re-trigger the sync effect forever.
/// * The JS side keeps a single editor instance, so mounting a second one
///   destroys the first. Only one pane may therefore contain this component at
///   a time — `EditorPane` guarantees that by switching modes rather than
///   hiding a pane with CSS.
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
    ViewRaw,
    ViewSplit,
    ViewFormatted,
    ToggleView,
    NewFile,
}

impl EditorCommand {
    /// `true` when this command only makes sense with the editor visible.
    ///
    /// Guarding these keeps a shortcut from writing into a hidden, destroyed
    /// CodeMirror instance.
    pub fn needs_editor(self) -> bool {
        matches!(
            self,
            Self::Bold
                | Self::Italic
                | Self::InlineCode
                | Self::Strikethrough
                | Self::Link
                | Self::Focus
        )
    }

    /// Run the command.
    pub fn run(self, state: EditorState, on_save: Callback<()>) {
        if self.needs_editor() && !self.is_editing(state) {
            return;
        }
        match self {
            Self::Save => on_save.run(()),
            Self::Bold => cm_toggle_wrap("**"),
            Self::Italic => cm_toggle_wrap("*"),
            Self::InlineCode => cm_toggle_wrap("`"),
            Self::Strikethrough => cm_toggle_wrap("~~"),
            Self::Link => cm_insert_link(),
            Self::Focus => cm_focus(),
            Self::ViewRaw => state.view_mode.set(ViewMode::Raw),
            Self::ViewSplit => state.view_mode.set(ViewMode::Split),
            Self::ViewFormatted => state.view_mode.set(ViewMode::Formatted),
            Self::ToggleView => {
                state.view_mode.set(match state.view_mode.get_untracked() {
                    ViewMode::Raw => ViewMode::Split,
                    _ => ViewMode::Raw,
                });
            }
            Self::NewFile => match state.path.get_untracked() {
                Some(folder) => crate::actions::create_file(state, folder),
                None => state.notify("Abrí una carpeta antes de crear un archivo"),
            },
        }
    }

    fn is_editing(self, state: EditorState) -> bool {
        matches!(
            state.view_mode.get_untracked(),
            ViewMode::Raw | ViewMode::Split
        )
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
        EditorCommand::ViewRaw => (true, "1"),
        EditorCommand::ViewSplit => (true, "3"),
        EditorCommand::ViewFormatted => (true, "2"),
        EditorCommand::ToggleView => (true, "0"),
        EditorCommand::NewFile => (true, "n"),
    })
}
