mod codemirror;

pub use codemirror::CodeMirrorEditor;

use crate::components::layout::ViewMode;
use leptos::prelude::*;

#[component]
pub fn EditorPane(
    editor_content: ReadSignal<String>,
    set_editor_content: WriteSignal<String>,
    preview_html: ReadSignal<String>,
    view_mode: ReadSignal<ViewMode>,
    editor_ratio: ReadSignal<f64>,
    set_is_resizing_editor: WriteSignal<bool>,
    on_save: Callback<()>,
) -> impl IntoView {
    let (is_dark, set_is_dark) = signal(false);

    Effect::new(move |_| {
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            if let Some(el) = doc.document_element() {
                set_is_dark.set(el.class_list().contains("dark"));
            }
        }
    });

    let show_editor = move || view_mode.get() != ViewMode::Reader;
    let show_preview = move || view_mode.get() != ViewMode::Source;

    view! {
        <div class="flex-1 flex overflow-hidden w-full h-full">
            {move || if show_editor() {
                view! {
                    <div
                        class=move || format!(
                            "flex flex-col border-r border-slate-200 dark:border-slate-800 relative group overflow-hidden {}",
                            if view_mode.get() == ViewMode::Source { "flex-1 min-w-0" } else { "flex-shrink-0" }
                        )
                        style:width=move || if view_mode.get() == ViewMode::Source { "".to_string() } else { format!("{}%", editor_ratio.get() * 100.0) }
                    >
                        <div class="absolute top-4 right-4 text-[10px] font-mono text-slate-300 dark:text-slate-700 pointer-events-none group-hover:opacity-100 opacity-0 transition-opacity z-10">
                            "EDITOR"
                        </div>
                        <CodeMirrorEditor
                            content=editor_content
                            set_content=set_editor_content
                            is_dark=is_dark
                            on_save=on_save
                        />
                    </div>
                    {move || if show_preview() {
                        view! {
                            <div
                                class="w-1 hover:w-1.5 bg-transparent hover:bg-brand-orange/40 cursor-col-resize transition-all z-50 flex-shrink-0"
                                on:mousedown=move |_| set_is_resizing_editor.set(true)
                            />
                        }.into_any()
                    } else {
                        view! { <div></div> }.into_any()
                    }}
                }.into_any()
            } else {
                view! { <div></div> }.into_any()
            }}

            {move || if show_preview() {
                view! {
                    <div class="flex-1 flex flex-col bg-slate-50/50 dark:bg-slate-900/10 relative group overflow-hidden min-w-0">
                        <div class="absolute top-4 right-4 text-[10px] font-mono text-slate-300 dark:text-slate-700 pointer-events-none group-hover:opacity-100 opacity-0 transition-opacity">
                            "PREVIEW"
                        </div>
                        <div class="flex-1 overflow-y-auto p-8 custom-scrollbar overflow-x-auto">
                            <div
                                class="prose dark:prose-invert prose-slate max-w-none break-words  prose-headings:font-bold prose-h1:text-3xl prose-h1:mb-6  prose-p:text-slate-600 dark:prose-p:text-slate-400 prose-p:leading-7  prose-pre:overflow-x-auto  prose-code:text-brand-orange prose-code:bg-brand-orange/10 prose-code:px-1 prose-code:py-0.5 prose-code:rounded prose-code:before:content-none prose-code:after:content-none  prose-blockquote:border-brand-orange/50 prose-blockquote:bg-brand-orange/5  prose-img:rounded-lg prose-img:shadow-md"
                                inner_html=preview_html
                            ></div>
                        </div>
                    </div>
                }.into_any()
            } else {
                view! { <div></div> }.into_any()
            }}
        </div>
    }
}
