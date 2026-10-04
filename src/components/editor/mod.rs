pub mod codemirror;

pub use codemirror::{CodeMirrorEditor, EditorCommand};

use leptos::prelude::*;
use leptos::reactive::spawn_local;
use wasm_bindgen::prelude::*;

use crate::state::{EditorState, ViewMode};

#[wasm_bindgen]
extern "C" {
    /// Runs the KaTeX/Mermaid pass and resolves when it has settled.
    ///
    /// Awaited (rather than fire-and-forget) so a rejected diagram surfaces as
    /// a handled rejection instead of an unhandled promise in the console.
    #[wasm_bindgen(js_name = __codedocs_render_enhancements_async, catch)]
    async fn render_enhancements_async() -> Result<JsValue, JsValue>;
}

/// The editor / preview pane.
///
/// In `Split` mode both panes are mounted. Switching to a single-pane mode
/// unmounts the other, so the CodeMirror instance is destroyed and rebuilt on
/// the next switch rather than being kept alive off-screen with a stale buffer.
#[component]
pub fn EditorPane(
    state: EditorState,
    preview_html: RwSignal<String>,
    on_save: Callback<()>,
) -> impl IntoView {
    view! {
        <div class="flex-1 overflow-hidden w-full h-full relative">
            {move || match state.view_mode.get() {
                ViewMode::Raw => view! {
                    <div class="w-full h-full overflow-hidden">
                        <CodeMirrorEditor state on_save />
                    </div>
                }
                .into_any(),

                ViewMode::Formatted => view! {
                    <div class="w-full h-full overflow-y-auto p-8 custom-scrollbar">
                        <Preview html=preview_html />
                    </div>
                }
                .into_any(),

                ViewMode::Split => view! {
                    <div class="w-full h-full flex flex-row">
                        <div class="flex-1 min-w-0 overflow-hidden border-r \
                                    border-base-200 dark:border-base-800">
                            <CodeMirrorEditor state on_save />
                        </div>
                        <div class="flex-1 min-w-0 overflow-y-auto p-8 custom-scrollbar">
                            <Preview html=preview_html />
                        </div>
                    </div>
                }
                .into_any(),
            }}
        </div>
    }
}

/// Rendered preview.
///
/// `inner_html` is safe here because `codedocs_core::render_markdown` drops
/// every HTML event in the document and emits only markup it generated itself.
/// The KaTeX/Mermaid pass runs after the HTML lands in the DOM.
#[component]
fn Preview(html: RwSignal<String>) -> impl IntoView {
    Effect::new(move |_| {
        let _ = html.get();
        spawn_local(async move {
            // Yield one microtask so the `inner_html` binding has been written
            // to the DOM before the bridge scans for placeholders. The effect
            // is tracked before Leptos flushes the text node, so without this
            // the pass can observe the previous document and leave this one's
            // math and diagrams unrendered.
            yield_to_browser().await;

            if let Err(err) = render_enhancements_async().await {
                leptos::logging::warn!("Falla al renderizar el preview: {err:?}");
            }
        });
    });

    view! {
        <div class="prose dark:prose-invert prose-slate max-w-none break-words" inner_html=html />
    }
}

/// Resolve on the next microtask.
async fn yield_to_browser() {
    let resolved = js_sys::Promise::resolve(&JsValue::NULL);
    let _ = wasm_bindgen_futures::JsFuture::from(resolved).await;
}
