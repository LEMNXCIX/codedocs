use leptos::prelude::*;

#[component]
pub fn EditorPane(
    editor_content: ReadSignal<String>,
    set_editor_content: WriteSignal<String>,
    preview_html: ReadSignal<String>,
    show_editor: ReadSignal<bool>,
    editor_ratio: ReadSignal<f64>,
    set_is_resizing_editor: WriteSignal<bool>,
    editor_ref: NodeRef<leptos::html::Textarea>,
) -> impl IntoView {
    view! {
        <div class="flex-1 flex overflow-hidden w-full h-full">
            {move || if show_editor.get() {
                view! {
                    <div
                        class="flex flex-col border-r border-slate-200 dark:border-slate-800 relative group animate-in slide-in-from-left duration-300 overflow-hidden flex-shrink-0"
                        style:width=move || format!("{}%", editor_ratio.get() * 100.0)
                    >
                        <div class="absolute top-4 right-4 text-[10px] font-mono text-slate-300 dark:text-slate-700 pointer-events-none group-hover:opacity-100 opacity-0 transition-opacity">
                            "EDITOR"
                        </div>
                        <textarea
                            node_ref=editor_ref
                            class="flex-1 w-full p-8 bg-transparent focus:outline-none resize-none font-mono text-sm leading-relaxed text-slate-700 dark:text-slate-300 selection:bg-brand-orange/20 custom-scrollbar whitespace-pre-wrap break-words"
                            placeholder="Empieza a escribir aquí..."
                            prop:value=move || editor_content.get()
                            on:input=move |ev| set_editor_content.set(event_target_value(&ev))
                        ></textarea>
                    </div>
                    <div
                        class="w-1 hover:w-1.5 bg-transparent hover:bg-brand-orange/40 cursor-col-resize transition-all z-50 flex-shrink-0"
                        on:mousedown=move |_| set_is_resizing_editor.set(true)
                    />
                }.into_any()
            } else {
                ().into_any()
            }}

            <div class="flex-1 flex flex-col bg-slate-50/50 dark:bg-slate-900/10 relative group overflow-hidden min-w-0">
                <div class="absolute top-4 right-4 text-[10px] font-mono text-slate-300 dark:text-slate-700 pointer-events-none group-hover:opacity-100 opacity-0 transition-opacity">
                    "PREVIEW"
                </div>
                <div class="flex-1 overflow-y-auto p-8 custom-scrollbar overflow-x-auto">
                    <div
                        class="prose dark:prose-invert prose-slate max-w-none break-words
                            prose-headings:font-bold prose-h1:text-3xl prose-h1:mb-6
                            prose-p:text-slate-600 dark:prose-p:text-slate-400 prose-p:leading-7
                            prose-pre:overflow-x-auto
                            prose-code:text-brand-orange prose-code:bg-brand-orange/10 prose-code:px-1 prose-code:py-0.5 prose-code:rounded prose-code:before:content-none prose-code:after:content-none
                            prose-blockquote:border-brand-orange/50 prose-blockquote:bg-brand-orange/5
                            prose-img:rounded-lg prose-img:shadow-md"
                        inner_html=preview_html
                    ></div>
                </div>
            </div>
        </div>
    }
}
