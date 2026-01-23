use leptos::prelude::*;

#[component]
pub fn AlertModal(
    title: String,
    message: String,
    on_confirm: Callback<()>,
    on_cancel: Callback<()>,
) -> impl IntoView {
    let _ = window_event_listener(leptos::ev::keydown, move |ev: leptos::ev::KeyboardEvent| {
        if ev.key() == "Escape" {
            on_cancel.run(());
        }
    });

    view! {
        <div class="fixed inset-0 z-[100] flex items-center justify-center bg-slate-900/50 backdrop-blur-sm p-4">
            <div class="bg-white dark:bg-slate-900 w-full max-w-md p-6 rounded-lg shadow-2xl border border-slate-200 dark:border-slate-800 animate-in zoom-in-95 duration-200">
                <h3 class="text-lg font-bold text-slate-900 dark:text-white mb-2">
                    {title}
                </h3>
                <p class="text-sm text-slate-500 dark:text-slate-400 mb-6">
                    {message}
                </p>

                <div class="flex justify-end gap-3">
                    <button
                        on:click=move |_| on_cancel.run(())
                        class="px-4 py-2 text-sm font-medium text-slate-600 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-800 rounded-md transition-colors"
                    >
                        "Cancelar"
                    </button>
                    <button
                        on:click=move |_| on_confirm.run(())
                        class="px-4 py-2 text-sm font-medium text-white bg-slate-900 dark:bg-white dark:text-slate-900 hover:opacity-90 rounded-md shadow-sm transition-colors"
                    >
                        "Confirmar"
                    </button>
                </div>
            </div>
        </div>
    }
}

#[component]

pub fn DeleteConfirmModal(
    path: String,
    on_confirm: Callback<()>,
    on_cancel: Callback<()>,
) -> impl IntoView {
    let _ = window_event_listener(leptos::ev::keydown, move |ev: leptos::ev::KeyboardEvent| {
        if ev.key() == "Escape" {
            on_cancel.run(());
        }
    });

    view! {
        <div class="fixed inset-0 z-[100] flex items-center justify-center bg-slate-900/50 backdrop-blur-sm p-4">
            <div class="bg-white dark:bg-slate-900 w-full max-w-md p-6 rounded-lg shadow-2xl border border-slate-200 dark:border-slate-800 animate-in zoom-in-95 duration-200">
                <h3 class="text-lg font-bold text-slate-900 dark:text-white mb-2">
                    "¿Eliminar archivo?"
                </h3>
                <p class="text-sm text-slate-500 dark:text-slate-400 mb-6">
                    "Estás a punto de borrar " <span class="font-mono text-xs">{path.clone()}</span>
                    ". Esta acción no se puede deshacer."
                </p>

                <div class="flex justify-end gap-3">
                    <button
                        on:click=move |_| on_cancel.run(())
                        class="px-4 py-2 text-sm font-medium text-slate-600 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-800 rounded-md transition-colors"
                    >
                        "Cancelar"
                    </button>
                    <button
                        on:click=move |_| on_confirm.run(())
                        class="px-4 py-2 text-sm font-medium text-white bg-red-600 hover:bg-red-700 rounded-md shadow-sm transition-colors"
                    >
                        "Eliminar permanentemente"
                    </button>
                </div>
            </div>
        </div>
    }
}

#[component]
pub fn RenameConfirmModal(
    path: String,
    on_confirm: Callback<String>,
    on_cancel: Callback<()>,
) -> impl IntoView {
    let initial_name = path
        .replace('\\', "/")
        .split('/')
        .last()
        .unwrap_or(&path)
        .to_string();
    let (new_name, set_new_name) = signal(initial_name);

    let _ = window_event_listener(leptos::ev::keydown, move |ev: leptos::ev::KeyboardEvent| {
        if ev.key() == "Escape" {
            on_cancel.run(());
        }
    });

    view! {
        <div class="fixed inset-0 z-[100] flex items-center justify-center bg-slate-900/50 backdrop-blur-sm p-4">
            <div class="bg-white dark:bg-slate-900 w-full max-w-md p-6 rounded-lg shadow-2xl border border-slate-200 dark:border-slate-800 animate-in zoom-in-95 duration-200">
                <h3 class="text-lg font-bold text-slate-900 dark:text-white mb-2">
                    "Renombrar archivo"
                </h3>
                <p class="text-xs text-slate-500 dark:text-slate-400 mb-4 truncate">
                    "Ruta: " {path.clone()}
                </p>

                <input
                    type="text"
                    class="w-full px-3 py-2 bg-slate-50 dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 mb-6 text-slate-900 dark:text-slate-100"
                    prop:value=move || new_name.get()
                    on:input=move |ev| set_new_name.set(event_target_value(&ev))
                    on:keydown=move |ev| {
                        if ev.key() == "Enter" {
                            on_confirm.run(new_name.get());
                        }
                    }
                />

                <div class="flex justify-end gap-3">
                    <button
                        on:click=move |_| on_cancel.run(())
                        class="px-4 py-2 text-sm font-medium text-slate-600 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-800 rounded-md transition-colors"
                    >
                        "Cancelar"
                    </button>
                    <button
                        on:click=move |_| on_confirm.run(new_name.get())
                        class="px-4 py-2 text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 rounded-md shadow-sm transition-colors"
                    >
                        "Guardar cambios"
                    </button>
                </div>
            </div>
        </div>
    }
}
