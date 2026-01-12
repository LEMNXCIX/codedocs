use leptos::prelude::*;
use pulldown_cmark::{Parser, html};

#[component]
pub fn EditorContainer() -> impl IntoView {
    let (content, set_content) = signal(String::from("# Hola\nEmpieza a escribir..."));

    // Memo para el HTML renderizado (Eficiencia pura)
    let preview_html = Memo::new(move |_| {
        let mut html_output = String::new();
        let content_str = content.get();
        let parser = Parser::new(&content_str);
        html::push_html(&mut html_output, parser);
        html_output
    });

    view! {
        <div class="grid grid-cols-1 md:grid-cols-2 gap-4 h-[calc(100vh-12rem)]">
            // Editor (Izquierda)
            <div class="flex flex-col bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-lg overflow-hidden shadow-sm">
                <div class="bg-slate-50 dark:bg-slate-800 px-4 py-2 border-b border-slate-200 dark:border-slate-700 text-[10px] font-bold uppercase tracking-widest text-slate-400">
                    "Editor de Markdown"
                </div>
                <textarea 
                    class="flex-1 p-6 bg-transparent resize-none focus:outline-none font-mono text-sm leading-relaxed text-slate-700 dark:text-slate-300"
                    on:input=move |ev| set_content.set(event_target_value(&ev))
                    prop:value=content
                />
            </div>

            // Preview (Derecha)
            <div class="flex flex-col bg-slate-50 dark:bg-slate-900/50 border border-slate-200 dark:border-slate-800 rounded-lg overflow-hidden shadow-sm">
                <div class="bg-slate-50 dark:bg-slate-800 px-4 py-2 border-b border-slate-200 dark:border-slate-700 text-[10px] font-bold uppercase tracking-widest text-slate-400">
                    "Vista Previa"
                </div>
                <div 
                    class="flex-1 p-8 overflow-auto prose dark:prose-invert prose-slate max-w-none"
                    inner_html=preview_html
                />
            </div>
        </div>
    }
}