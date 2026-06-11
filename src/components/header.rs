use leptos::prelude::*;

#[component]
pub fn EditorHeader(
    selected_file: ReadSignal<Option<String>>,
) -> impl IntoView {
    view! {
        <div class="h-12 border-b border-slate-200 dark:border-slate-800 flex items-center px-4 bg-white dark:bg-brand-dark/50 backdrop-blur-md z-10">
            <span class="text-xs font-mono text-slate-400 dark:text-slate-500 truncate">
                {move || selected_file.get().unwrap_or_else(|| "Sin archivo seleccionado".to_string())}
            </span>
        </div>
    }
}
