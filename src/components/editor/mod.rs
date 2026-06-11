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

    view! {
        <div class="flex-1 overflow-hidden w-full h-full relative">
            {move || match view_mode.get() {
                ViewMode::Raw => view! {
                    <div class="w-full h-full overflow-hidden">
                        <CodeMirrorEditor
                            content=editor_content
                            set_content=set_editor_content
                            is_dark=is_dark
                            on_save=on_save
                        />
                    </div>
                }.into_any(),
                ViewMode::Formatted => view! {
                    <div class="w-full h-full flex flex-col bg-slate-50/50 dark:bg-slate-900/10 overflow-hidden">
                        <div class="flex-1 overflow-y-auto p-8 custom-scrollbar overflow-x-auto">
                            <div
                                class="prose dark:prose-invert prose-slate max-w-none break-words prose-headings:font-bold prose-h1:text-3xl prose-h1:mb-6 prose-p:text-slate-600 dark:prose-p:text-slate-400 prose-p:leading-7 prose-pre:overflow-x-auto prose-code:text-brand-orange prose-code:bg-brand-orange/10 prose-code:px-1 prose-code:py-0.5 prose-code:rounded prose-code:before:content-none prose-code:after:content-none prose-blockquote:border-brand-orange/50 prose-blockquote:bg-brand-orange/5 prose-img:rounded-lg prose-img:shadow-md"
                                inner_html=preview_html
                            ></div>
                        </div>
                    </div>
                }.into_any(),
            }}
        </div>
    }
}
