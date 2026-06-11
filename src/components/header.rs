use leptos::prelude::*;

#[component]
pub fn EditorHeader(
    selected_file: ReadSignal<Option<String>>,
) -> impl IntoView {
    view! {
        <div class="h-12 border-b border-base-200 dark:border-base-800 flex items-center px-4 bg-base-50 dark:bg-base-900/50 backdrop-blur-md z-10">
            <span class="text-xs font-mono text-base-400 dark:text-base-500 truncate">
                {move || selected_file.get().unwrap_or_else(|| "Sin archivo seleccionado".to_string())}
            </span>
        </div>
    }
}
