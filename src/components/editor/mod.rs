pub mod codemirror;

pub use codemirror::{CodeMirrorEditor, EditorCommand};

use leptos::prelude::*;

use crate::state::EditorState;

/// The editor pane.
///
/// There is a single editing mode: the live-format editor owns the document.
/// No read-only preview is mounted anywhere — the markdown is the document,
/// so there is nothing to keep in sync and no per-keystroke HTML render.
#[component]
pub fn EditorPane(state: EditorState, on_save: Callback<()>) -> impl IntoView {
    view! {
        <div class="flex-1 overflow-hidden w-full h-full relative">
            <div class="w-full h-full overflow-hidden">
                <CodeMirrorEditor state on_save />
            </div>
        </div>
    }
}
