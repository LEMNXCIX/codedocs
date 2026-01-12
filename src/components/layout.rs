use crate::components::ui::{Button, List};
use leptos::children::Children;
use leptos::logging::error;
use leptos::prelude::*;
use leptos::reactive::spawn_local;
use leptos::*;
use wasm_bindgen::prelude::*;
use js_sys;
use web_sys;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch)]
    async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

#[component]
pub fn OpenFolderButton(
    set_files: WriteSignal<Vec<String>>,
    set_path: WriteSignal<String>,
) -> impl IntoView {
    let fn_open_folder = move |_: leptos::ev::MouseEvent| {
        spawn_local(async move {
            let result = invoke("open_project_folder", JsValue::null()).await;

            match result {
                Ok(path_js) => {
                    // Si es Ok, debería ser un string
                    if let Some(path_str) = path_js.as_string() {
                        set_path.set(path_str.clone());

                        spawn_local(async move {
                            let args = js_sys::Object::new();
                            js_sys::Reflect::set(&args, &"folderPath".into(), &JsValue::from(path_str)).unwrap();
                            let files = invoke("list_markdown_files", args.into()).await;
                            set_files.update(|f| {
                                *f = match files {
                                    Ok(files_js) => {
                                        if let Some(array) = files_js.dyn_ref::<js_sys::Array>() {
                                            array
                                                .iter()
                                                .filter_map(|item| item.as_string())
                                                .collect()
                                        } else {
                                            vec!["ERROR: Respuesta inválida al listar archivos"
                                                .to_string()]
                                        }
                                    }
                                    Err(err_js) => {
                                        let error_msg = err_js.as_string().unwrap_or_else(|| {
                                            "Error desconocido al listar archivos".to_string()
                                        });
                                        error!(
                                            "Error al listar archivos desde Tauri: {}",
                                            error_msg
                                        );
                                        vec![format!("ERROR: {}", error_msg)]
                                    }
                                }
                            });
                        });
                    } else {
                        // Caso raro: no es string
                        set_path.set("ERROR: Respuesta inválida del backend".to_string());
                        error!("Respuesta Ok no es string: {:?}", path_js);
                    }
                }
                Err(err_js) => {
                    // Aquí llega el mensaje de error del backend como string
                    let error_msg = err_js
                        .as_string()
                        .unwrap_or_else(|| "Error desconocido al abrir carpeta".to_string());

                    error!("Error desde Tauri: {}", error_msg);
                    set_path.set(format!("ERROR: {}", error_msg));
                }
            }
        });
    };

    view! {
        <div>
            <Button on_click=fn_open_folder>
                "Abrir carpeta del proyecto"
            </Button>
        </div>
    }
}

#[component]
pub fn Layout(children: Children) -> impl IntoView {
    let (path, set_path) = signal(String::from("No se ha seleccionado ninguna carpeta"));
    let (files, set_files) = signal(Vec::<String>::new());
    let (editor_content, set_editor_content) = signal(String::new());
    let (preview_html, set_preview_html) = signal(String::new());

    // Effect to update preview when content changes
    Effect::new(move |_| {
        let content = editor_content.get();
        spawn_local(async move {
            let args = js_sys::Object::new();
            js_sys::Reflect::set(&args, &"content".into(), &JsValue::from(content)).unwrap();
            let html = invoke("render_markdown", args.into()).await;
            match html {
                Ok(html_js) => {
                    if let Some(html_str) = html_js.as_string() {
                        set_preview_html.set(html_str);
                    }
                }
                Err(_) => set_preview_html.set("<p>Error rendering markdown</p>".to_string()),
            }
        });
    });

    let on_file_click = Callback::new(move |file_path: String| {
        let full_path = format!("{}/{}", path.get(), file_path);
        spawn_local(async move {
            let args = js_sys::Object::new();
            js_sys::Reflect::set(&args, &"pathStr".into(), &JsValue::from(full_path)).unwrap();
            let content = invoke("read_file", args.into()).await;
            match content {
                Ok(content_js) => {
                    if let Some(content_str) = content_js.as_string() {
                        set_editor_content.set(content_str);
                    }
                }
                Err(err) => {
                    error!("Error reading file: {:?}", err);
                }
            }
        });
    });

    let clear_editor = move |_| {
        let window = web_sys::window().unwrap();
        if window.confirm_with_message("¿Estás seguro de que quieres limpiar el editor?").unwrap_or(false) {
            set_editor_content.set(String::new());
        }
    };

    view! {
        <div class="flex flex-row h-screen bg-slate-50 dark:bg-brand-dark text-slate-900 dark:text-slate-200 font-sans transition-all duration-300">
            <aside class="w-64 border-r border-slate-200 dark:border-slate-800 bg-slate-50 dark:bg-brand-dark p-4 overflow-y-auto z-10 transition-all duration-300">
                <div
                    class="group flex items-center gap-3 mb-8 cursor-pointer select-none"
                    on:dblclick=move |_| {
                        let doc = leptos::prelude::document().document_element().unwrap();
                        let _ = doc.class_list().toggle("dark");
                    }
                >
                    <svg width="32" height="32" viewBox="0 0 512 512" fill="none" xmlns="http://www.w3.org/2000/svg" class="w-8 h-8 opacity-90 group-hover:opacity-100 transition-opacity">
                        <rect width="512" height="512" rx="120" fill="#121212"/>
                        <path d="m110 176 80 80-80 80" stroke="#4fc3f7" stroke-width="45" stroke-linecap="round" stroke-linejoin="round"/>
                        <path d="m290 130-70 252" stroke="#ffb74d" stroke-width="45" stroke-linecap="round"/>
                        <path d="m402 176-80 80 80 80" stroke="#4fc3f7" stroke-width="45" stroke-linecap="round" stroke-linejoin="round"/>
                    </svg>
                    <div class="flex flex-col justify-center">
                        <h1 class="text-xl font-semibold tracking-tight text-slate-900 dark:text-gray-100 leading-none">
                            "CodeDocs"
                        </h1>
                    </div>
                </div>
                <h2 class="text-[10px] font-bold uppercase tracking-widest text-slate-400 dark:text-slate-500 mb-4 px-1">
                    Explorador
                </h2>
                <OpenFolderButton set_files=set_files set_path=set_path/>
                <div class="space-y-2">
                    <div class="p-2 rounded-lg hover:bg-slate-100 dark:hover:bg-slate-800 cursor-pointer transition-colors text-sm font-medium text-slate-700 dark:text-slate-300 hover:text-brand-orange">
                        Documentos
                    </div>
                    <p class="text-xs text-slate-500 truncate mt-2">{move || path.get()}</p>
                    <List items=files on_click=on_file_click/>
                </div>
            </aside>
            <main class="flex-1 grid grid-cols-2 gap-4 p-4 overflow-y-auto bg-slate-50 dark:bg-brand-dark transition-all duration-300">
                <div class="bg-white dark:bg-slate-800 p-4 rounded-lg shadow">
                    <h3 class="text-lg font-semibold mb-2">"Editor"</h3>
                    <textarea
                        class="w-full h-96 p-2 border rounded resize-none"
                        prop:value=editor_content
                        on:input=move |ev| set_editor_content.set(event_target_value(&ev))
                    ></textarea>
                    <button
                        class="mt-2 px-4 py-2 bg-red-500 text-white rounded hover:bg-red-600"
                        on:click=clear_editor
                    >
                        "Limpiar"
                    </button>
                </div>
                <div class="bg-white dark:bg-slate-800 p-4 rounded-lg shadow">
                    <h3 class="text-lg font-semibold mb-2">"Vista Previa"</h3>
                    <div class="prose max-w-none" inner_html=preview_html></div>
                </div>
            </main>
        </div>
    }
}
