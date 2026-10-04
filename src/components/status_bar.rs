use crate::actions;
use crate::state::EditorState;
use leptos::prelude::*;

/// Status bar: document stats and save indicator.
#[component]
pub fn StatusBar(state: EditorState, on_save: Callback<()>) -> impl IntoView {
    view! {
        <footer class="h-8 border-t border-base-200 dark:border-base-800 flex items-center \
                      justify-between px-4 bg-base-50 dark:bg-base-900/80 backdrop-blur-md \
                      z-20 flex-shrink-0">
            <div class="flex items-center gap-3">
                <span class="text-[10px] font-mono text-base-400 dark:text-base-600">
                    {move || {
                        let content = state.content.get();
                        format!(
                            "{} palabras · {} caracteres",
                            crate::state::word_count(&content),
                            crate::state::char_count(&content)
                        )
                    }}
                </span>
            </div>

            <div class="flex items-center gap-2">
                <SaveIndicator state />
                <button
                    class="flex items-center gap-1.5 px-2.5 py-1 rounded-md text-[11px] \
                           font-medium transition-all bg-base-900 hover:bg-base-700 \
                           text-base-50 disabled:opacity-50 disabled:cursor-not-allowed"
                    disabled=move || !crate::utils::env::is_tauri()
                        || state.save_state.get() == crate::state::SaveState::Idle
                    title="Guardar cambios (Ctrl+S)"
                    on:click=move |_| on_save.run(())
                >
                    <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24"
                        fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"
                        stroke-linejoin="round"><path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z"/>
                        <polyline points="17 21 17 13 7 13 7 21"/><polyline points="7 3 7 8 15 8"/></svg>
                    "Guardar"
                </button>
                <button
                    class="p-1 text-base-400 hover:text-brand-orange hover:bg-base-100 \
                           dark:hover:bg-base-800 rounded-md transition-all"
                    title="Limpiar editor"
                    on:click=move |_| actions::clear_editor(state)
                >
                    <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24"
                        fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"
                        stroke-linejoin="round"><path d="M3 6h18"/><path d="M19 6v14c1 0 2-1 2-2V6"/>
                        <path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/>
                        <line x1="10" y1="11" x2="10" y2="17"/><line x1="14" y1="11" x2="14" y2="17"/></svg>
                </button>
            </div>
        </footer>
    }
}

/// Shows whether the buffer matches disk.
#[component]
fn SaveIndicator(state: EditorState) -> impl IntoView {
    view! {
        <Show when=move || {
            let save_state = state.save_state.get();
            !matches!(save_state, crate::state::SaveState::Idle)
        }>
            <span
                class="text-[10px] font-medium"
                class:text-brand-orange=move || state.save_state.get().is_error()
                class:text-base-400=move || !state.save_state.get().is_error()
            >
                {move || state.save_state.get().label()}
            </span>
        </Show>
    }
}
