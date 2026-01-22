use crate::components::ui::{Button, FileEntry, FileTree};
use js_sys;
use leptos::logging::error;
use leptos::prelude::*;
use leptos::reactive::spawn_local;
use wasm_bindgen::prelude::*;
use web_sys;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch)]
    async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

#[component]
pub fn OpenFolderButton(
    set_files: WriteSignal<Vec<FileEntry>>,
    set_path: WriteSignal<String>,
) -> impl IntoView {
    let fn_open_folder = move |_: leptos::ev::MouseEvent| {
        spawn_local(async move {
            let result = invoke("open_project_folder", JsValue::null()).await;

            match result {
                Ok(path_js) => {
                    if let Some(path_str) = path_js.as_string() {
                        set_path.set(path_str.clone());

                        spawn_local(async move {
                            let args = js_sys::Object::new();
                            js_sys::Reflect::set(
                                &args,
                                &"folderPath".into(),
                                &JsValue::from(path_str),
                            )
                            .unwrap();
                            let files_result = invoke("list_markdown_files", args.into()).await;

                            match files_result {
                                Ok(files_js) => {
                                    match serde_wasm_bindgen::from_value::<Vec<FileEntry>>(files_js)
                                    {
                                        Ok(tree) => set_files.set(tree),
                                        Err(err) => {
                                            error!("Error deserializing file tree: {:?}", err);
                                        }
                                    }
                                }
                                Err(err_js) => {
                                    let error_msg = err_js.as_string().unwrap_or_else(|| {
                                        "Error desconocido al listar archivos".to_string()
                                    });
                                    error!("Error al listar archivos desde Tauri: {}", error_msg);
                                }
                            }
                        });
                    }
                }
                Err(err_js) => {
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
pub fn Layout() -> impl IntoView {
    let (path, set_path) = signal(String::from("No se ha seleccionado ninguna carpeta"));
    let (files, set_files) = signal(Vec::<FileEntry>::new());
    let (editor_content, set_editor_content) = signal(String::new());
    let (preview_html, set_preview_html) = signal(String::new());
    let (show_editor, set_show_editor) = signal(false);

    // Resizing State
    let (sidebar_width, set_sidebar_width) = signal(280.0);
    let (editor_ratio, set_editor_ratio) = signal(0.5);
    let (is_resizing_sidebar, set_is_resizing_sidebar) = signal(false);
    let (is_resizing_editor, set_is_resizing_editor) = signal(false);

    // Global Mouse Handlers for Resizing
    let _ = window_event_listener(leptos::ev::mousemove, move |ev: leptos::ev::MouseEvent| {
        if is_resizing_sidebar.get() {
            let new_width = ev.client_x() as f64;
            if new_width > 160.0 && new_width < 500.0 {
                set_sidebar_width.set(new_width);
            }
        }
        if is_resizing_editor.get() {
            let start_x = sidebar_width.get();
            let total_width = web_sys::window()
                .unwrap()
                .inner_width()
                .unwrap()
                .as_f64()
                .unwrap();
            let main_width = total_width - start_x - 10.0; // Subtract resizer space
            let current_x = ev.client_x() as f64 - start_x;
            let ratio = current_x / main_width;
            if ratio > 0.1 && ratio < 0.9 {
                set_editor_ratio.set(ratio);
            }
        }
    });

    let _ = window_event_listener(leptos::ev::mouseup, move |_| {
        set_is_resizing_sidebar.set(false);
        set_is_resizing_editor.set(false);
    });

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

    let on_file_click = Callback::new(move |full_path: String| {
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
        if window
            .confirm_with_message("¿Estás seguro de que quieres limpiar el editor?")
            .unwrap_or(false)
        {
            set_editor_content.set(String::new());
        }
    };

    view! {
        <div class="flex flex-row h-screen bg-slate-50 dark:bg-brand-dark text-slate-900 dark:text-slate-200 font-sans transition-all duration-300 overflow-hidden"
             class:select-none=move || is_resizing_sidebar.get() || is_resizing_editor.get()
             class:cursor-col-resize=move || is_resizing_sidebar.get() || is_resizing_editor.get()>

            // Sidebar
            <aside
                class="border-r border-slate-200 dark:border-slate-800 bg-slate-50 dark:bg-brand-dark flex flex-col transition-none flex-shrink-0 overflow-hidden"
                style:width=move || format!("{}px", sidebar_width.get())
            >
                <div class="p-6 border-b border-slate-200 dark:border-slate-800">
                    <div
                        class="group flex items-center gap-3 cursor-pointer select-none mb-6"
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
                        <h1 class="text-xl font-bold tracking-tight text-slate-900 dark:text-gray-100 italic">
                            "CodeDocs"
                        </h1>
                    </div>

                    <h2 class="text-[10px] font-bold uppercase tracking-widest text-slate-400 dark:text-slate-500 mb-4 px-1">
                        "Explorador"
                    </h2>
                    <OpenFolderButton set_files=set_files set_path=set_path/>
                </div>

                <div class="flex-1 overflow-y-auto p-4 custom-scrollbar">
                    <div class="mb-4">
                        <p class="text-[11px] font-mono text-slate-400 dark:text-slate-600 truncate bg-slate-100 dark:bg-slate-900/50 p-2 rounded border border-slate-200 dark:border-slate-800">
                            {move || path.get()}
                        </p>
                    </div>
                    <FileTree items=files on_click=on_file_click/>
                </div>
            </aside>

            // Resizer Sidebar
            <div
                class="w-1 hover:w-1.5 bg-transparent hover:bg-brand-orange/40 cursor-col-resize transition-all z-50 flex-shrink-0"
                on:mousedown=move |_| set_is_resizing_sidebar.set(true)
            />

            // Main Content Area
            <main class="flex-1 flex flex-col min-w-0 bg-white dark:bg-brand-dark overflow-hidden">
                <div class="h-14 border-b border-slate-200 dark:border-slate-800 flex items-center justify-between px-6 bg-white dark:bg-brand-dark/50 backdrop-blur-md z-10">
                    <div class="flex items-center gap-4">
                        <span class="text-sm font-medium text-slate-500 dark:text-slate-400">{move || if show_editor.get() { "Markdown Editor" } else { "Markdown Viewer" }}</span>
                        <div class="h-4 w-px bg-slate-200 dark:border-slate-800"></div>
                        <span class="text-xs font-mono text-brand-orange">
                            {move || if editor_content.get().is_empty() { "Empty" } else { "Document Loaded" }}
                        </span>
                    </div>
                    <div class="flex items-center gap-2">
                        // Toggle Editor Button
                        <button
                            class=move || format!(
                                "flex items-center gap-2 px-3 py-1.5 rounded-md text-xs font-medium transition-all {}",
                                if show_editor.get() {
                                    "bg-slate-900 text-white dark:bg-white dark:text-slate-900"
                                } else {
                                    "bg-slate-100 text-slate-600 hover:bg-slate-200 dark:bg-slate-800 dark:text-slate-300 dark:hover:bg-slate-700"
                                }
                            )
                            on:click=move |_| set_show_editor.update(|v| *v = !*v)
                        >
                            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/>
                                <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/>
                            </svg>
                            {move || if show_editor.get() { "Ocultar Editor" } else { "Editar" }}
                        </button>

                         <button
                            class="p-2 text-slate-400 hover:text-red-500 hover:bg-red-50 dark:hover:bg-red-500/10 rounded-md transition-all group"
                            title="Limpiar editor"
                            on:click=clear_editor
                        >
                            <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/><line x1="10" y1="11" x2="10" y2="17"/><line x1="14" y1="11" x2="14" y2="17"/></svg>
                        </button>
                    </div>
                </div>

                // Split Pane
                <div class="flex-1 flex overflow-hidden w-full h-full">
                    // Editor Pane (Conditional)
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
                                    class="flex-1 w-full p-8 bg-transparent focus:outline-none resize-none font-mono text-sm leading-relaxed text-slate-700 dark:text-slate-300 selection:bg-brand-orange/20 custom-scrollbar whitespace-pre-wrap break-words"
                                    placeholder="Empieza a escribir aquí..."
                                    prop:value=editor_content
                                    on:input=move |ev| set_editor_content.set(event_target_value(&ev))
                                ></textarea>
                            </div>
                            // Resizer Editor
                            <div
                                class="w-1 hover:w-1.5 bg-transparent hover:bg-brand-orange/40 cursor-col-resize transition-all z-50 flex-shrink-0"
                                on:mousedown=move |_| set_is_resizing_editor.set(true)
                            />
                        }.into_any()
                    } else {
                        ().into_any()
                    }}

                    // Preview Pane
                    <div class="flex-1 flex flex-col bg-slate-50/50 dark:bg-slate-900/10 relative group overflow-hidden min-w-0">
                        <div class="absolute top-4 right-4 text-[10px] font-mono text-slate-300 dark:text-slate-700 pointer-events-none group-hover:opacity-100 opacity-0 transition-opacity">
                            "PREVIEW"
                        </div>
                        <div class="flex-1 overflow-y-auto p-8 custom-scrollbar overflow-x-hidden">
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
            </main>
        </div>
    }
}
