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
            class="border-r border-base-200 dark:border-base-800 bg-base-100 dark:bg-base-900 flex flex-col transition-none flex-shrink-0 overflow-hidden"
            style:width=move || format!("{}px", sidebar_width.get())
        >
            <div class="p-6 border-b border-base-200 dark:border-base-800">
                <div
                    class="group flex items-center gap-3 cursor-pointer select-none mb-6"
                    on:dblclick=move |_| {
                        let doc = leptos::prelude::document().document_element().unwrap();
                        let _ = doc.class_list().toggle("dark");
                    }
                >
                    <svg width="32" height="32" viewBox="0 0 1000 1000" fill="none" xmlns="http://www.w3.org/2000/svg" class="w-8 h-8 opacity-90 group-hover:opacity-100 transition-opacity flex-shrink-0">
                        <g transform="matrix(0.994487,-0.104858,0.104858,0.994487,-94.013334,48.040307)">
                            <path d="M923.276,183.448L923.276,860.431C923.276,904.775 887.275,940.776 842.931,940.776L200.172,940.776C155.829,940.776 119.828,904.775 119.828,860.431L119.828,183.448C119.828,139.105 155.829,103.103 200.172,103.103L842.931,103.103C887.275,103.103 923.276,139.105 923.276,183.448Z" style="fill:rgb(252,255,255);stroke:rgb(142,142,142);stroke-opacity:0.24;stroke-width:4.17px;"/>
                        </g>
                        <g transform="matrix(1,0,0,1,24.002123,7.817214)">
                            <g transform="matrix(1,0,0,1,-2.643528,-61.595632)">
                                <path d="M923.276,183.448L923.276,860.431C923.276,904.775 887.275,940.776 842.931,940.776L200.172,940.776C155.829,940.776 119.828,904.775 119.828,860.431L119.828,183.448C119.828,139.105 155.829,103.103 200.172,103.103L842.931,103.103C887.275,103.103 923.276,139.105 923.276,183.448Z" style="fill:rgb(252,255,255);stroke:rgb(141,141,141);stroke-opacity:0.24;stroke-width:4.17px;"/>
                            </g>
                            <g transform="matrix(1,0,0,0.884146,1.390949,121.707249)">
                                <path d="M860.69,164.448C860.69,176.933 851.728,187.069 840.69,187.069L194.345,187.069C183.307,187.069 174.345,176.933 174.345,164.448C174.345,151.964 183.307,141.828 194.345,141.828L840.69,141.828C851.728,141.828 860.69,151.964 860.69,164.448Z" style="fill:rgb(243,243,243);"/>
                            </g>
                            <g transform="matrix(1,0,0,0.884146,1.390949,246.83121)">
                                <path d="M860.69,164.448C860.69,176.933 851.728,187.069 840.69,187.069L194.345,187.069C183.307,187.069 174.345,176.933 174.345,164.448C174.345,151.964 183.307,141.828 194.345,141.828L840.69,141.828C851.728,141.828 860.69,151.964 860.69,164.448Z" style="fill:rgb(243,243,243);"/>
                            </g>
                            <g transform="matrix(1,0,0,0.884146,1.390949,361.141567)">
                                <path d="M860.69,164.448C860.69,176.933 851.728,187.069 840.69,187.069L194.345,187.069C183.307,187.069 174.345,176.933 174.345,164.448C174.345,151.964 183.307,141.828 194.345,141.828L840.69,141.828C851.728,141.828 860.69,151.964 860.69,164.448Z" style="fill:rgb(243,243,243);"/>
                            </g>
                            <g transform="matrix(1,0,0,0.884146,1.390949,594.189815)">
                                <path d="M860.69,164.448C860.69,176.933 851.728,187.069 840.69,187.069L194.345,187.069C183.307,187.069 174.345,176.933 174.345,164.448C174.345,151.964 183.307,141.828 194.345,141.828L840.69,141.828C851.728,141.828 860.69,151.964 860.69,164.448Z" style="fill:rgb(243,243,243);"/>
                            </g>
                            <g transform="matrix(1,0,0,0.884146,1.390949,483.624274)">
                                <path d="M860.69,164.448C860.69,176.933 851.728,187.069 840.69,187.069L194.345,187.069C183.307,187.069 174.345,176.933 174.345,164.448C174.345,151.964 183.307,141.828 194.345,141.828L840.69,141.828C851.728,141.828 860.69,151.964 860.69,164.448Z" style="fill:rgb(243,243,243);"/>
                            </g>
                            <g transform="matrix(1,0,0,0.884146,1.390949,7.412977)">
                                <path d="M860.69,164.448C860.69,176.933 851.728,187.069 840.69,187.069L194.345,187.069C183.307,187.069 174.345,176.933 174.345,164.448C174.345,151.964 183.307,141.828 194.345,141.828L840.69,141.828C851.728,141.828 860.69,151.964 860.69,164.448Z" style="fill:rgb(242,242,242);"/>
                            </g>
                            <g transform="matrix(0.544512,0,0,0.544512,-18.662162,377.007267)">
                                <text x="362.069px" y="450.431px" style="font-family:'UnifrakturMaguntia', sans-serif;font-size:833.416px;fill:rgb(47,47,47);">CD</text>
                            </g>
                        </g>
                    </svg>
                    <h1 class="text-xl tracking-tight text-base-900 dark:text-base-100 font-unifraktur">
                        "CodeDocs"
                    </h1>
                </div>

                <div class="mb-6">
                    <AppStatus />
                </div>

                <div class="flex flex-col gap-2">
                    <div class="flex gap-1 bg-base-100 dark:bg-base-800/50 rounded-md p-0.5">
                        <button
                            class=move || format!(
                                "flex-1 text-[10px] font-bold uppercase tracking-wider py-1.5 rounded transition-colors {}",
                                if active_tab.get() == SidebarTab::Files { "bg-base-50 dark:bg-base-700 text-base-900 dark:text-base-50 shadow-sm" } else { "text-base-500 dark:text-base-400 hover:text-base-700 dark:hover:text-base-300" }
                            )
                            on:click=move |_| set_active_tab.set(SidebarTab::Files)
                        >
                            "Archivos"
                        </button>
                        <button
                            class=move || format!(
                                "flex-1 text-[10px] font-bold uppercase tracking-wider py-1.5 rounded transition-colors {}",
                                if active_tab.get() == SidebarTab::Outline { "bg-base-50 dark:bg-base-700 text-base-900 dark:text-base-50 shadow-sm" } else { "text-base-500 dark:text-base-400 hover:text-base-700 dark:hover:text-base-300" }
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
                                            class="flex items-center gap-2 px-3 py-1.5 bg-base-100 hover:bg-base-200 dark:bg-base-800 dark:hover:bg-base-700 text-base-700 dark:text-base-300 rounded-md text-xs font-medium transition-all"
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
                                <p class="text-[11px] font-mono text-base-400 dark:text-base-600 truncate bg-base-100 dark:bg-base-900/50 p-2 rounded border border-base-200 dark:border-base-800">
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
