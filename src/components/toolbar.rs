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
        ("API Doc", "# 📡 Endpoint\n\n**GET** `/api/v1/resource`\n"),
        ("Nota Rápida", "> [!NOTE]\n> \n"),
        ("Checklist", "- [ ] Tarea 1\n- [ ] Tarea 2\n"),
    ];

    view! {
        <div class="flex gap-2 p-2 bg-slate-50 dark:bg-slate-800 border-b border-slate-200 dark:border-slate-700">
            <span class="text-[10px] font-bold text-slate-400 uppercase flex items-center px-2">
                "Insertar:"
            </span>
            {
                templates.into_iter().map(|(name, text)| {
                    view!{
                        <button
                            class="px-2 py-1 rounded-md bg-slate-100 dark:bg-slate-700 hover:bg-slate-200 dark:hover:bg-slate-600"
                            on:click=move |_| {
                                set_content.update(|content| content.push_str(text));
                                if let Some(el) = editor_ref.get() {
                                    let _ = el.focus();
                                }
                            }
                        >
                            {name}
                        </button>
                    }
                }).collect_view()
            }
            <div class="h-4 w-px bg-slate-200 dark:bg-slate-700 mx-2"></div>
            <button
                class="px-2 py-1 rounded-md bg-brand-orange/10 text-brand-orange hover:bg-brand-orange/20 font-medium text-xs transition-colors"
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
                "Generar Índice"
            </button>
        </div>
    }
}
