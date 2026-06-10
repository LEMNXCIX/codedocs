mod outline;

use crate::types::FileEntry;
use crate::utils::env::is_tauri;
use crate::utils::tauri_bridge::{self, invoke};
use crate::components::ui::{AppStatus, Button, FileTree};
use crate::utils::markdown::Heading;
use leptos::logging::error;
use leptos::prelude::*;
use leptos::reactive::spawn_local;
use wasm_bindgen::JsValue;

#[component]
pub fn OpenFolderButton(
    set_files: WriteSignal<Vec<FileEntry>>,
    set_path: WriteSignal<String>,
) -> impl IntoView {
    let fn_open_folder = move |_: leptos::ev::MouseEvent| {
        spawn_local(async move {
            if !is_tauri() {
                set_path.set("C:\\Demo\\Documents".to_string());
                let mock_files = vec![
                    FileEntry {
                        name: "Bienvenido.md".to_string(),
                        path: "C:\\Demo\\Documents\\Bienvenido.md".to_string(),
                        is_dir: false,
                        children: vec![],
                    },
                    FileEntry {
                        name: "Guía_Rápida.md".to_string(),
                        path: "C:\\Demo\\Documents\\Guía_Rápida.md".to_string(),
                        is_dir: false,
                        children: vec![],
                    },
                    FileEntry {
                        name: "Proyectos".to_string(),
                        path: "C:\\Demo\\Documents\\Proyectos".to_string(),
                        is_dir: true,
                        children: vec![FileEntry {
                            name: "Demo.md".to_string(),
                            path: "C:\\Demo\\Documents\\Proyectos\\Demo.md".to_string(),
                            is_dir: false,
                            children: vec![],
                        }],
                    },
                ];
                set_files.set(mock_files);
                return;
            }

            let result = invoke("open_project_folder", JsValue::null()).await;

            match result {
                Ok(path_js) => {
                    if let Some(path_str) = path_js.as_string() {
                        set_path.set(path_str.clone());

                        spawn_local(async move {
                            let args = tauri_bridge::args_with("folderPath", &path_str);
                            let files_result = invoke("list_markdown_files", args).await;

                            match files_result {
                                Ok(files_js) => {
                                    match serde_wasm_bindgen::from_value::<Vec<FileEntry>>(files_js) {
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

#[derive(Clone, Copy, PartialEq)]
pub enum SidebarTab {
    Files,
    Outline,
}

#[component]
pub fn Sidebar(
    path: ReadSignal<String>,
    files: ReadSignal<Vec<FileEntry>>,
    set_files: WriteSignal<Vec<FileEntry>>,
    set_path: WriteSignal<String>,
    sidebar_width: ReadSignal<f64>,
    on_file_click: Callback<String>,
    on_delete: Callback<String>,
    on_rename: Callback<String>,
    create_new_file: Callback<()>,
    headings: ReadSignal<Vec<Heading>>,
) -> impl IntoView {
    let (active_tab, set_active_tab) = signal(SidebarTab::Files);

    view! {
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
                    <h1 class="text-xl font-bold tracking-tight text-slate-900 dark:text-gray-100">
                        "CodeDocs"
                    </h1>
                </div>

                <div class="mb-6">
                    <AppStatus />
                </div>

                <div class="flex flex-col gap-2">
                    <div class="flex gap-1 bg-slate-100 dark:bg-slate-800/50 rounded-md p-0.5">
                        <button
                            class=move || format!(
                                "flex-1 text-[10px] font-bold uppercase tracking-wider py-1.5 rounded transition-colors {}",
                                if active_tab.get() == SidebarTab::Files { "bg-white dark:bg-slate-700 text-slate-900 dark:text-white shadow-sm" } else { "text-slate-500 dark:text-slate-400 hover:text-slate-700 dark:hover:text-slate-300" }
                            )
                            on:click=move |_| set_active_tab.set(SidebarTab::Files)
                        >
                            "Archivos"
                        </button>
                        <button
                            class=move || format!(
                                "flex-1 text-[10px] font-bold uppercase tracking-wider py-1.5 rounded transition-colors {}",
                                if active_tab.get() == SidebarTab::Outline { "bg-white dark:bg-slate-700 text-slate-900 dark:text-white shadow-sm" } else { "text-slate-500 dark:text-slate-400 hover:text-slate-700 dark:hover:text-slate-300" }
                            )
                            on:click=move |_| set_active_tab.set(SidebarTab::Outline)
                        >
                            "Contenido"
                        </button>
                    </div>

                    {move || if active_tab.get() == SidebarTab::Files {
                        view! {
                            <div class="flex flex-col gap-2 mt-2">
                                <OpenFolderButton set_files=set_files set_path=set_path />
                                {if is_tauri() {
                                    view! {
                                        <button
                                            class="flex items-center gap-2 px-3 py-1.5 bg-slate-100 hover:bg-slate-200 dark:bg-slate-800 dark:hover:bg-slate-700 text-slate-700 dark:text-slate-300 rounded-md text-xs font-medium transition-all"
                                            on:click=move |_| create_new_file.run(())
                                        >
                                            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 5v14M5 12h14"/></svg>
                                            "Nuevo Archivo"
                                        </button>
                    }.into_any()
                } else {
                    view! { <div></div> }.into_any()
                }}
                </div>
            }.into_any()
            } else {
                view! { <div></div> }.into_any()
            }}
                </div>
            </div>

            <div class="flex-1 overflow-y-auto p-4 custom-scrollbar">
                {move || if active_tab.get() == SidebarTab::Files {
                    view! {
                        <div>
                            <div class="mb-4">
                                <p class="text-[11px] font-mono text-slate-400 dark:text-slate-600 truncate bg-slate-100 dark:bg-slate-900/50 p-2 rounded border border-slate-200 dark:border-slate-800">
                                    {move || path.get()}
                                </p>
                            </div>
                            <FileTree items=files on_click=on_file_click on_delete=on_delete on_rename=on_rename />
                        </div>
                    }.into_any()
                } else {
                    view! {
                        <outline::OutlinePanel headings=headings />
                    }.into_any()
                }}
            </div>
        </aside>
    }
}
