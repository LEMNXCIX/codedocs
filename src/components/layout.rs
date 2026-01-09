use leptos::children::Children;
use leptos::prelude::*;
use leptos::reactive::spawn_local;
use leptos::*;
use leptos::logging::error;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch)]
    async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

#[component]
pub fn OpenFolderButton() -> impl IntoView {
    let (path, set_path) = signal(String::from("No se ha seleccionado ninguna carpeta"));

    let fn_open_folder = move |_| {
        spawn_local(async move {
            let result = invoke("open_project_folder", JsValue::null()).await;

            match result {
                Ok(path_js) => {
                    // Si es Ok, debería ser un string
                    if let Some(path_str) = path_js.as_string() {
                        set_path.set(path_str);
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
            <button class="px-4 py-2 bg-slate-900 hover:bg-slate-800 dark:bg-white dark:text-slate-900 dark:hover:bg-slate-200 text-white rounded-md text-sm font-medium transition-colors" 
                on:click=fn_open_folder>
                "Abrir carpeta"
            </button>
            <p class="text-xs text-slate-500 truncate mt-2">{move || path.get()}</p>
        </div>
    }
}

#[component]
pub fn Layout(children: Children) -> impl IntoView {
    view! {
        <div class="flex flex-row h-screen bg-slate-50 dark:bg-brand-dark text-slate-900 dark:text-slate-200 font-sans transition-colors duration-300">
            <aside class="w-64 border-r border-slate-200 dark:border-slate-800 bg-slate-50 dark:bg-brand-dark p-4 overflow-y-auto z-10">
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
                <div class="space-y-2">
                    <div class="p-2 rounded-lg hover:bg-slate-100 dark:hover:bg-slate-800 cursor-pointer transition-colors text-sm font-medium text-slate-700 dark:text-slate-300 hover:text-brand-orange">
                        Documentos
                    </div>
                </div>
            </aside>
            <main class="flex-1 p-8 overflow-y-auto bg-slate-50 dark:bg-brand-dark">
                {children()}
            </main>
        </div>
    }
}
