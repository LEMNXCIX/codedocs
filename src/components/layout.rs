use crate::components::modals::{AlertModal, DeleteConfirmModal, RenameConfirmModal};
use crate::components::sidebar::Sidebar;
use crate::components::editor::EditorPane;
use crate::components::header::EditorHeader;
use crate::types::FileEntry;
use crate::utils::env::is_tauri;
use crate::utils::markdown::{extract_headings, render_markdown, Heading};
use crate::utils::tauri_bridge::{self, invoke};
use wasm_bindgen::{JsCast, JsValue};
use leptos::logging::error;
use leptos::prelude::*;
use leptos::reactive::spawn_local;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ViewMode {
    Raw,
    Formatted,
}

#[component]
pub fn Layout() -> impl IntoView {
    let (path, set_path) = signal(String::from("No se ha seleccionado ninguna carpeta"));
    let (files, set_files) = signal(Vec::<FileEntry>::new());
    let (selected_file, set_selected_file) = signal::<Option<String>>(None);
    let (editor_content, set_editor_content) = signal(String::new());
    let (preview_html, set_preview_html) = signal(String::new());
    let (view_mode, set_view_mode) = signal(ViewMode::Formatted);
    let is_saving = RwSignal::new(false);
    let is_loading_file = RwSignal::new(false);
    let (headings, set_headings) = signal(Vec::<Heading>::new());

    let (sidebar_width, set_sidebar_width) = signal(280.0);
    let (is_resizing_sidebar, set_is_resizing_sidebar) = signal(false);

    let (file_to_delete, set_file_to_delete) = signal::<Option<String>>(None);
    let (file_to_rename, set_file_to_rename) = signal::<Option<String>>(None);
    let (show_clear_confirm, set_show_clear_confirm) = signal(false);

    let _ = window_event_listener(leptos::ev::mousemove, move |ev: leptos::ev::MouseEvent| {
        if is_resizing_sidebar.get() {
            let new_width = ev.client_x() as f64;
            if new_width > 160.0 && new_width < 500.0 {
                set_sidebar_width.set(new_width);
            }
        }
    });

    let _ = window_event_listener(leptos::ev::mouseup, move |_| {
        set_is_resizing_sidebar.set(false);
    });

    let _ = window_event_listener(leptos::ev::keydown, move |ev: leptos::ev::KeyboardEvent| {
        let key = ev.key();
        let ctrl = ev.ctrl_key() || ev.meta_key();
        if ctrl && key == "1" {
            ev.prevent_default();
            set_view_mode.set(ViewMode::Raw);
        } else if ctrl && key == "2" {
            ev.prevent_default();
            set_view_mode.set(ViewMode::Formatted);
        }
    });

    Effect::new(move |_| {
        let content = editor_content.get();
        let html_output = render_markdown(&content);
        set_preview_html.set(html_output);
        set_headings.set(extract_headings(&content));

        spawn_local(async move {
            if let Some(window) = web_sys::window() {
                let _ = js_sys::Reflect::get(&window, &JsValue::from_str("__codedocs_render_enhancements"))
                    .ok()
                    .and_then(|f| js_sys::Function::from(f).call0(&window).ok());
            }
        });
    });

    Effect::new(move |_| {
        let current_path = path.get();
        if current_path == "No se ha seleccionado ninguna carpeta" || !is_tauri() {
            return;
        }
        let fp = current_path.clone();
        spawn_local(async move {
            let args = tauri_bridge::args_with("folderPath", &fp);
            let _ = invoke("watch_folder", args).await;
        });
    });

    let refresh_files = move || {
        let current_path = path.get();
        if current_path != "No se ha seleccionado ninguna carpeta" {
            spawn_local(async move {
                let args = tauri_bridge::args_with("folderPath", &current_path);
                let files_result = invoke("list_markdown_files", args).await;
                if let Ok(files_js) = files_result {
                    if let Ok(tree) = serde_wasm_bindgen::from_value::<Vec<FileEntry>>(files_js) {
                        set_files.set(tree);
                    }
                }
            });
        }
    };

    {
        let set_editor_content = set_editor_content.clone();
        let selected_file = selected_file.clone();
        let refresh_files_ref = refresh_files.clone();
        let is_saving = is_saving.clone();

        let on_fs_change = wasm_bindgen::closure::Closure::<dyn Fn(JsValue)>::new(
            move |event: JsValue| {
                if is_saving.get() {
                    return;
                }
                if let Some(payload) = js_sys::Reflect::get(&event, &JsValue::from_str("payload")).ok() {
                    if let Some(changed_paths) = payload.as_string() {
                        let current_file = selected_file.get();
                        if let Some(ref current) = current_file {
                            if changed_paths.contains(current) {
                                let fp = current.clone();
                                let set_ec = set_editor_content.clone();
                                spawn_local(async move {
                                    let args = tauri_bridge::args_with("pathStr", &fp);
                                    if let Ok(content_js) = invoke("read_file", args).await {
                                        if let Some(content) = content_js.as_string() {
                                            set_ec.set(content);
                                        }
                                    }
                                });
                            }
                        }
                        refresh_files_ref();
                    }
                }
            },
        );

        spawn_local(async move {
            tauri_bridge::listen("fs-change", on_fs_change.as_ref().unchecked_ref()).await;
            on_fs_change.forget();
        });
    }

    let on_delete_request = Callback::new(move |file_path: String| {
        set_file_to_delete.set(Some(file_path));
    });

    let on_rename_request = Callback::new(move |file_path: String| {
        set_file_to_rename.set(Some(file_path));
    });

    let handle_delete_confirm = move |_| {
        if let Some(file_path) = file_to_delete.get() {
            spawn_local(async move {
                let args = tauri_bridge::args_with("pathStr", &file_path);
                let result = invoke("delete_file", args).await;
                match result {
                    Ok(_) => {
                        set_file_to_delete.set(None);
                        refresh_files();
                    }
                    Err(err) => {
                        error!("Error deleting file: {:?}", err);
                    }
                }
            });
        }
    };

    let handle_rename_confirm = move |new_name: String| {
        if let Some(old_path) = file_to_rename.get() {
            spawn_local(async move {
                let args = js_sys::Object::new();
                tauri_bridge::set_arg(&args, "oldPath", JsValue::from(old_path.clone()));
                tauri_bridge::set_arg(&args, "newName", JsValue::from(new_name));
                let result = invoke("rename_file", args.into()).await;
                match result {
                    Ok(_) => {
                        set_file_to_rename.set(None);
                        refresh_files();
                    }
                    Err(err) => {
                        error!("Error renaming file: {:?}", err);
                    }
                }
            });
        }
    };

    let create_new_file = Callback::new(move |_| {
        let current_path = path.get();
        if current_path != "No se ha seleccionado ninguna carpeta" {
            spawn_local(async move {
                let args = js_sys::Object::new();
                tauri_bridge::set_arg(&args, "folderPath", JsValue::from(current_path.clone()));
                tauri_bridge::set_arg(&args, "name", JsValue::from("Nuevo_Documento.md"));
                let result = invoke("create_file", args.into()).await;
                match result {
                    Ok(new_path_js) => {
                        refresh_files();
                        if let Some(new_path) = new_path_js.as_string() {
                            leptos::logging::log!("Created: {}", new_path);
                        }
                    }
                    Err(err) => error!("Error creating file: {:?}", err),
                }
            });
        }
    });

    let on_save = Callback::new(move |_| {
        if let Some(file_path) = selected_file.get() {
            if is_tauri() {
                let content = editor_content.get();
                is_saving.set(true);
                spawn_local(async move {
                    let args = js_sys::Object::new();
                    tauri_bridge::set_arg(&args, "pathStr", JsValue::from(file_path));
                    tauri_bridge::set_arg(&args, "content", JsValue::from(content));
                    let _ = invoke("save_file", args.into()).await;
                    is_saving.set(false);
                });
            }
        }
    });

    let (auto_save_timer_id, set_auto_save_timer_id) = signal::<Option<i32>>(None);

    Effect::new(move |_| {
        let content = editor_content.get();
        let file_path = selected_file.get();
        if !is_tauri() || is_loading_file.get() {
            return;
        }
        let Some(fp) = file_path else { return };

        if let Some(id) = auto_save_timer_id.get_untracked() {
            if let Some(window) = web_sys::window() {
                let _ = window.clear_timeout_with_handle(id);
            }
        }

        let window = match web_sys::window() {
            Some(w) => w,
            None => return,
        };

        let content_clone = content.clone();
        let fp_clone = fp.clone();
        let is_saving_clone = is_saving.clone();
        let closure = wasm_bindgen::closure::Closure::once(move || {
            is_saving_clone.set(true);
            spawn_local(async move {
                let args = js_sys::Object::new();
                tauri_bridge::set_arg(&args, "pathStr", JsValue::from(fp_clone));
                tauri_bridge::set_arg(&args, "content", JsValue::from(content_clone));
                let _ = invoke("save_file", args.into()).await;
                leptos::logging::log!("Auto-guardado");
                is_saving_clone.set(false);
            });
        });

        let id = window
            .set_timeout_with_callback_and_timeout_and_arguments_0(
                closure.as_ref().unchecked_ref(),
                2000,
            )
            .unwrap_or(0);
        closure.forget();
        set_auto_save_timer_id.set(Some(id));
    });

    let on_file_click = Callback::new(move |full_path: String| {
        leptos::logging::log!("on_file_click: {}", full_path);
        is_loading_file.set(true);
        set_selected_file.set(Some(full_path.clone()));
        spawn_local(async move {
            if !is_tauri() {
                let mock_content = match full_path.as_str() {
                    "C:\\Demo\\Documents\\Bienvenido.md" => "# 👋 Bienvenido a CodeDocs\n\nEsta es una **demo interactiva** en la web.\n\n### Características:\n- Edición rápida\n- Previsualización en tiempo real\n- Soporte para plantillas",
                    "C:\\Demo\\Documents\\Guía_Rápida.md" => "# ⚡ Guía Rápida\n\n1. Selecciona un archivo.\n2. Edita su contenido.\n3. Mira la preview a la derecha.",
                    _ => "# 📂 Archivo Demo\n\nContenido de ejemplo para la versión web.",
                };
                set_editor_content.set(mock_content.to_string());
                is_loading_file.set(false);
                return;
            }
            leptos::logging::log!("Leyendo archivo: {}", full_path);
            let args = tauri_bridge::args_with("pathStr", &full_path);
            let content = invoke("read_file", args).await;
            leptos::logging::log!("read_file resultado: {:?}", content.as_ref().map(|v| v.as_string().map(|s| s.len())));
            match content {
                Ok(content_js) => {
                    if let Some(content_str) = content_js.as_string() {
                        leptos::logging::log!("Contenido cargado: {} chars", content_str.len());
                        set_editor_content.set(content_str);
                    } else {
                        leptos::logging::log!("WARN: read_file devolvió non-string");
                    }
                }
                Err(err) => {
                    error!("Error reading file: {:?}", err);
                }
            }
            is_loading_file.set(false);
            leptos::logging::log!("is_loading_file = false");
        });
    });

    let clear_editor = Callback::new(move |_| {
        set_show_clear_confirm.set(true);
    });

    let handle_clear_confirm = move |_| {
        set_editor_content.set(String::new());
        set_show_clear_confirm.set(false);
    };

    let word_count = move || {
        let content = editor_content.get();
        if content.trim().is_empty() {
            0
        } else {
            content.split_whitespace().count()
        }
    };

    let char_count = move || editor_content.get().len();

    view! {
        <div class="flex flex-col h-screen bg-base-50 dark:bg-base-900 text-base-900 dark:text-base-200 font-sans transition-all duration-300 overflow-hidden">

            <div class="flex flex-row flex-1 min-h-0"
                class:select-none=move || is_resizing_sidebar.get()
                class:cursor-col-resize=move || is_resizing_sidebar.get()
            >

                <Sidebar
                    path=path
                    files=files
                    set_files=set_files
                    set_path=set_path
                    sidebar_width=sidebar_width
                    on_file_click=on_file_click
                    on_delete=on_delete_request
                    on_rename=on_rename_request
                    create_new_file=create_new_file
                    headings=headings
                />

                <div
                    class="w-1 hover:w-1.5 bg-transparent hover:bg-brand-orange/40 cursor-col-resize transition-all z-50 flex-shrink-0"
                    on:mousedown=move |_| set_is_resizing_sidebar.set(true)
                />

                <main class="flex-1 flex flex-col min-w-0 bg-base-50 dark:bg-base-900 overflow-hidden">
                    <EditorHeader selected_file=selected_file />

                    <EditorPane
                        editor_content=editor_content
                        set_editor_content=set_editor_content
                        preview_html=preview_html
                        view_mode=view_mode
                        on_save=on_save
                    />
                </main>
            </div>

            <footer class="h-8 border-t border-base-200 dark:border-base-800 flex items-center justify-between px-4 bg-base-50 dark:bg-base-900/80 backdrop-blur-md z-20 flex-shrink-0">
                <div class="flex items-center gap-3">
                    <div class="flex gap-0.5 bg-base-100 dark:bg-base-800/50 rounded-md p-0.5">
                        <button
                            class=move || format!(
                                "px-2.5 py-0.5 rounded text-[11px] font-medium transition-all {}",
                                if view_mode.get() == ViewMode::Raw {
                                    "bg-base-50 dark:bg-base-700 text-base-900 dark:text-base-50 shadow-sm"
                                } else {
                                    "text-base-500 dark:text-base-400 hover:text-base-700 dark:hover:text-base-300"
                                }
                            )
                            on:click=move |_| set_view_mode.set(ViewMode::Raw)
                        >
                            "Raw"
                        </button>
                        <button
                            class=move || format!(
                                "px-2.5 py-0.5 rounded text-[11px] font-medium transition-all {}",
                                if view_mode.get() == ViewMode::Formatted {
                                    "bg-base-50 dark:bg-base-700 text-base-900 dark:text-base-50 shadow-sm"
                                } else {
                                    "text-base-500 dark:text-base-400 hover:text-base-700 dark:hover:text-base-300"
                                }
                            )
                            on:click=move |_| set_view_mode.set(ViewMode::Formatted)
                        >
                            "Format"
                        </button>
                    </div>

                    <div class="h-3 w-px bg-base-200 dark:bg-base-800"></div>

                    <span class="text-[10px] font-mono text-base-400 dark:text-base-600">
                        {move || format!("{} palabras", word_count())}
                    </span>
                    <span class="text-[10px] font-mono text-base-400 dark:text-base-600">
                        {move || format!("{} caracteres", char_count())}
                    </span>
                </div>

                <div class="flex items-center gap-1">
                    <button
                        class="flex items-center gap-1.5 px-2.5 py-1 rounded-md text-[11px] font-medium transition-all bg-base-900 hover:bg-base-700 text-base-50 disabled:opacity-50 disabled:cursor-not-allowed"
                        disabled=move || !is_tauri()
                        title=move || if is_tauri() { "Guardar cambios" } else { "Guardado directo deshabilitado en versión web" }
                        on:click=move |_| on_save.run(())
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z"/><polyline points="17 21 17 13 7 13 7 21"/><polyline points="7 3 7 8 15 8"/></svg>
                        "Guardar"
                    </button>

                    <button
                        class="p-1 text-base-400 hover:text-brand-orange hover:bg-base-100 dark:hover:bg-base-800 rounded-md transition-all"
                        title="Limpiar editor"
                        on:click=move |_| clear_editor.run(())
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/><line x1="10" y1="11" x2="10" y2="17"/><line x1="14" y1="11" x2="14" y2="17"/></svg>
                    </button>
                </div>
            </footer>

            {move || file_to_delete.get().map(|path| {
                view! {
                    <DeleteConfirmModal
                        path=path
                        on_confirm=Callback::new(handle_delete_confirm)
                        on_cancel=Callback::new(move |_| set_file_to_delete.set(None))
                    />
                }
            })}

            {move || file_to_rename.get().map(|path| {
                view! {
                    <RenameConfirmModal
                        path=path
                        on_confirm=Callback::new(handle_rename_confirm)
                        on_cancel=Callback::new(move |_| set_file_to_rename.set(None))
                    />
                }
            })}

            {move || if show_clear_confirm.get() {
                view! {
                    <AlertModal
                        title="Limpiar editor".to_string()
                        message="¿Estás seguro de que quieres limpiar todo el contenido? Esta acción no se puede deshacer.".to_string()
                        on_confirm=Callback::new(handle_clear_confirm)
                        on_cancel=Callback::new(move |_| set_show_clear_confirm.set(false))
                    />
                }.into_any()
            } else {
                ().into_any()
            }}
        </div>
    }
}
