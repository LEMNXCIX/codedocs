use crate::components::modals::{AlertModal, DeleteConfirmModal, RenameConfirmModal};
use crate::components::sidebar::Sidebar;
use crate::components::editor::EditorPane;
use crate::components::header::EditorHeader;
use crate::types::FileEntry;
use crate::utils::env::is_tauri;
use crate::utils::tauri_bridge::{self, invoke};
use wasm_bindgen::JsValue;
use leptos::logging::error;
use leptos::prelude::*;
use leptos::reactive::spawn_local;

#[component]
pub fn Layout() -> impl IntoView {
    let (path, set_path) = signal(String::from("No se ha seleccionado ninguna carpeta"));
    let (files, set_files) = signal(Vec::<FileEntry>::new());
    let (selected_file, set_selected_file) = signal::<Option<String>>(None);
    let (editor_content, set_editor_content) = signal(String::new());
    let (preview_html, set_preview_html) = signal(String::new());
    let (show_editor, set_show_editor) = signal(false);

    let (sidebar_width, set_sidebar_width) = signal(280.0);
    let (editor_ratio, set_editor_ratio) = signal(0.5);
    let (is_resizing_sidebar, set_is_resizing_sidebar) = signal(false);
    let (is_resizing_editor, set_is_resizing_editor) = signal(false);
    let editor_ref = NodeRef::<leptos::html::Textarea>::new();

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
        if is_resizing_editor.get() {
            let start_x = sidebar_width.get();
            let total_width = web_sys::window()
                .unwrap()
                .inner_width()
                .unwrap()
                .as_f64()
                .unwrap();
            let main_width = total_width - start_x - 10.0;
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

    Effect::new(move |_| {
        let mut content = editor_content.get();

        content = content.replace("[!NOTE]", "**NOTE**");
        content = content.replace("[!TIP]", "**TIP**");
        content = content.replace("[!IMPORTANT]", "**IMPORTANT**");
        content = content.replace("[!WARNING]", "**WARNING**");
        content = content.replace("[!CAUTION]", "**CAUTION**");

        let mut options = pulldown_cmark::Options::empty();
        options.insert(pulldown_cmark::Options::ENABLE_TABLES);
        options.insert(pulldown_cmark::Options::ENABLE_FOOTNOTES);
        options.insert(pulldown_cmark::Options::ENABLE_STRIKETHROUGH);
        options.insert(pulldown_cmark::Options::ENABLE_TASKLISTS);
        options.insert(pulldown_cmark::Options::ENABLE_SMART_PUNCTUATION);
        options.insert(pulldown_cmark::Options::ENABLE_HEADING_ATTRIBUTES);

        let parser = pulldown_cmark::Parser::new_ext(&content, options);
        let mut html_output = String::new();
        pulldown_cmark::html::push_html(&mut html_output, parser);
        set_preview_html.set(html_output);
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

    let on_file_click = Callback::new(move |full_path: String| {
        set_selected_file.set(Some(full_path.clone()));
        spawn_local(async move {
            if !is_tauri() {
                let mock_content = match full_path.as_str() {
                    "C:\\Demo\\Documents\\Bienvenido.md" => "# 👋 Bienvenido a CodeDocs\n\nEsta es una **demo interactiva** en la web.\n\n### Características:\n- Edición rápida\n- Previsualización en tiempo real\n- Soporte para plantillas",
                    "C:\\Demo\\Documents\\Guía_Rápida.md" => "# ⚡ Guía Rápida\n\n1. Selecciona un archivo.\n2. Edita su contenido.\n3. Mira la preview a la derecha.",
                    _ => "# 📂 Archivo Demo\n\nContenido de ejemplo para la versión web.",
                };
                set_editor_content.set(mock_content.to_string());
                return;
            }
                let args = tauri_bridge::args_with("pathStr", &full_path);
            let content = invoke("read_file", args).await;
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

    let clear_editor = Callback::new(move |_| {
        set_show_clear_confirm.set(true);
    });

    let handle_clear_confirm = move |_| {
        set_editor_content.set(String::new());
        set_show_clear_confirm.set(false);
    };

    view! {
        <div class="flex flex-row h-screen bg-slate-50 dark:bg-brand-dark text-slate-900 dark:text-slate-200 font-sans transition-all duration-300 overflow-hidden"
            class:select-none=move || is_resizing_sidebar.get() || is_resizing_editor.get()
            class:cursor-col-resize=move || is_resizing_sidebar.get() || is_resizing_editor.get()>

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
        />

            // Resizer Sidebar
            <div
                class="w-1 hover:w-1.5 bg-transparent hover:bg-brand-orange/40 cursor-col-resize transition-all z-50 flex-shrink-0"
                on:mousedown=move |_| set_is_resizing_sidebar.set(true)
            />

            <main class="flex-1 flex flex-col min-w-0 bg-white dark:bg-brand-dark overflow-hidden">
                <EditorHeader
                    editor_content=editor_content
                    set_editor_content=set_editor_content
                    editor_ref=editor_ref
                    show_editor=show_editor
                    set_show_editor=set_show_editor
                    selected_file=selected_file
                    on_clear=clear_editor
                />

                <EditorPane
                    editor_content=editor_content
                    set_editor_content=set_editor_content
                    preview_html=preview_html
                    show_editor=show_editor
                    editor_ratio=editor_ratio
                    set_is_resizing_editor=set_is_resizing_editor
                    editor_ref=editor_ref
                />
            </main>

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
