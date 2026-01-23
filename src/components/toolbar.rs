use crate::utils::env::is_tauri;
use leptos::prelude::*;
use leptos::reactive::spawn_local;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch)]
    async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

#[component]
pub fn TemplateToolbar(
    content: ReadSignal<String>,
    set_content: WriteSignal<String>,
    editor_ref: NodeRef<leptos::html::Textarea>,
) -> impl IntoView {
    let templates = vec![
        (
            "API Doc",
            "# 📡 Endpoint\n\n**GET** `/api/v1/resource`\n",
            view! { <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="mr-1.5"><path d="m18 16 4-4-4-4"/><path d="m6 8-4 4 4 4"/><path d="m14.5 4-5 16"/></svg> }.into_any()
        ),
        (
            "Nota Rápida",
            "> [!NOTE]\n> \n",
            view! { <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="mr-1.5"><path d="M15.5 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V8.5L15.5 3z"/><path d="M15 3v6h6"/><path d="M9 18h6"/><path d="M9 14h6"/></svg> }.into_any()
        ),
        (
            "Checklist",
            "- [ ] Tarea 1\n- [ ] Tarea 2\n",
            view! { <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="mr-1.5"><rect width="18" height="18" x="3" y="3" rx="2"/><path d="m9 12 2 2 4-4"/></svg> }.into_any()
        ),
    ];

    view! {
        <div class="flex items-center gap-1.5 p-2 bg-white dark:bg-brand-dark/80 backdrop-blur-md border-b border-slate-200 dark:border-slate-800/60 sticky top-0 z-20">
            <div class="flex items-center px-3 border-r border-slate-200 dark:border-slate-800 mr-1">
                <span class="text-[10px] font-bold text-slate-400 dark:text-slate-500 uppercase tracking-tighter">
                    "Plantillas"
                </span>
            </div>

            <div class="flex items-center gap-1.5">
                {
                    templates.into_iter().map(|(name, text, icon)| {
                        view!{
                            <button
                                class="inline-flex items-center px-3 py-1.5 rounded-md text-xs font-medium text-slate-600 dark:text-slate-400 hover:text-slate-900 dark:hover:text-slate-100 hover:bg-slate-100 dark:hover:bg-slate-800/80 transition-all duration-200 active:scale-95 group"
                                on:click=move |_| {
                                    set_content.update(|content| {
                                        if !content.is_empty() && !content.ends_with('\n') {
                                            content.push('\n');
                                        }
                                        content.push_str(text);
                                    });
                                    if let Some(el) = editor_ref.get() {
                                        let _ = el.focus();
                                    }
                                }
                            >
                                {icon}
                                {name}
                            </button>
                        }
                    }).collect_view()
                }
            </div>

            <div class="h-5 w-px bg-slate-200 dark:bg-slate-800 mx-2"></div>

            <div class="flex-1 flex items-center">
                 <button
                    class="inline-flex items-center px-4 py-1.5 rounded-full bg-brand-orange/10 text-brand-orange hover:bg-brand-orange hover:text-white font-semibold text-[11px] transition-all duration-300 shadow-sm shadow-brand-orange/5 active:scale-95 disabled:opacity-50 disabled:cursor-not-allowed group relative"
                    disabled=move || !is_tauri()
                    title=move || if is_tauri() { "Generar tabla de contenidos" } else { "Generación de índice deshabilitada en versión web" }
                    on:click=move |_| {
                        let current_content = content.get();
                        spawn_local(async move {
                            let args = js_sys::Object::new();
                            js_sys::Reflect::set(&args, &"content".into(), &JsValue::from(current_content)).unwrap();
                            let result = invoke("generate_toc", args.into()).await;
                            if let Ok(toc_js) = result {
                                if let Some(toc) = toc_js.as_string() {
                                    set_content.update(|c| {
                                        if !c.is_empty() && !c.ends_with('\n') {
                                            c.push('\n');
                                        }
                                        c.push_str(&toc);
                                    });
                                    if let Some(el) = editor_ref.get() {
                                        let _ = el.focus();
                                    }
                                }
                            }
                        });
                    }
                >
                    <svg xmlns="http://www.w3.org/2000/svg" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" class="mr-2"><line x1="10" x2="21" y1="6" y2="6"/><line x1="10" x2="21" y1="12" y2="12"/><line x1="10" x2="21" y1="18" y2="18"/><path d="M4 6h1v4"/><path d="M4 10h2"/><path d="M6 18H4c0-1 2-2 2-3s-1-1.5-2-1"/></svg>
                    "Generar Índice"
                    {move || if !is_tauri() {
                        view! {
                            <span class="absolute bottom-full left-1/2 -translate-x-1/2 mb-2 px-2 py-1 bg-slate-800 text-white text-[10px] rounded opacity-0 group-hover:opacity-100 transition-opacity whitespace-nowrap pointer-events-none">
                                "Generación de índice deshabilitada en versión web"
                            </span>
                        }.into_any()
                    } else {
                        ().into_any()
                    }}
                </button>
            </div>


        </div>
    }
}
