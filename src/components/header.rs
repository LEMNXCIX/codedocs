use crate::utils::env::is_tauri;
use crate::utils::tauri_bridge::{self, invoke};
use crate::components::toolbar::TemplateToolbar;
use leptos::prelude::*;
use leptos::reactive::spawn_local;
use wasm_bindgen::JsValue;

#[component]
pub fn EditorHeader(
    editor_content: ReadSignal<String>,
    set_editor_content: WriteSignal<String>,
    editor_ref: NodeRef<leptos::html::Textarea>,
    show_editor: ReadSignal<bool>,
    set_show_editor: WriteSignal<bool>,
    selected_file: ReadSignal<Option<String>>,
    on_clear: Callback<()>,
) -> impl IntoView {
    view! {
        <TemplateToolbar content=editor_content set_content=set_editor_content editor_ref=editor_ref />
        <div class="h-14 border-b border-slate-200 dark:border-slate-800 flex items-center justify-between px-6 bg-white dark:bg-brand-dark/50 backdrop-blur-md z-10">
            <div class="flex items-center gap-4">
                <span class="text-sm font-medium text-slate-500 dark:text-slate-400">
                    {move || if show_editor.get() { "Markdown Editor" } else { "Markdown Viewer" }}
                </span>
                <div class="h-4 w-px bg-slate-200 dark:border-slate-800"></div>
                <span class="text-xs font-mono text-brand-orange">
                    {move || if editor_content.get().is_empty() { "Empty" } else { "Document Loaded" }}
                </span>
            </div>
            <div class="flex items-center gap-2">
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
                    class="flex items-center gap-2 px-3 py-1.5 rounded-md text-xs font-medium transition-all bg-green-600 hover:bg-green-700 text-white disabled:opacity-50 disabled:cursor-not-allowed group relative"
                    disabled=move || !is_tauri()
                    title=move || if is_tauri() { "Guardar cambios" } else { "Guardado directo deshabilitado en versión web" }
                    on:click=move |_| {
                        if let Some(file_path) = selected_file.get() {
                            let content = editor_content.get();
                            spawn_local(async move {
                                let args = js_sys::Object::new();
                                tauri_bridge::set_arg(&args, "pathStr", JsValue::from(file_path));
                                tauri_bridge::set_arg(&args, "content", JsValue::from(content));
                                let _ = invoke("save_file", args.into()).await;
                            });
                        }
                    }
                >
                    <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z"/><polyline points="17 21 17 13 7 13 7 21"/><polyline points="7 3 7 8 15 8"/>
                    </svg>
                    "Guardar"
                    {move || if !is_tauri() {
                        view! {
                            <span class="absolute bottom-full left-1/2 -translate-x-1/2 mb-2 px-2 py-1 bg-slate-800 text-white text-[10px] rounded opacity-0 group-hover:opacity-100 transition-opacity whitespace-nowrap pointer-events-none">
                                "Guardado directo deshabilitado en versión web"
                            </span>
                        }.into_any()
                    } else {
                        ().into_any()
                    }}
                </button>

                <button
                    class="p-2 text-slate-400 hover:text-red-500 hover:bg-red-50 dark:hover:bg-red-500/10 rounded-md transition-all group"
                    title="Limpiar editor"
                    on:click=move |_| on_clear.run(())
                >
                    <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/><line x1="10" y1="11" x2="10" y2="17"/><line x1="14" y1="11" x2="14" y2="17"/></svg>
                </button>
            </div>
        </div>
    }
}
