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
        <div class="fixed inset-0 z-[100] flex items-center justify-center bg-base-900/50 backdrop-blur-sm p-4">
            <div class="bg-base-50 dark:bg-base-900 w-full max-w-md p-6 rounded-lg shadow-2xl border border-base-200 dark:border-base-800 animate-in zoom-in-95 duration-200">
                <h3 class="text-lg font-bold text-base-900 dark:text-base-50 mb-2">
                    {title}
                </h3>
                <p class="text-sm text-base-500 dark:text-base-400 mb-6">
                    {message}
                </p>

                <div class="flex justify-end gap-3">
                    <button
                        on:click=move |_| on_cancel.run(())
                        class="px-4 py-2 text-sm font-medium text-base-600 dark:text-base-400 hover:bg-base-100 dark:hover:bg-base-800 rounded-md transition-colors"
                    >
                        "Cancelar"
                    </button>
                    <button
                        on:click=move |_| on_confirm.run(())
                        class="px-4 py-2 text-sm font-medium text-base-50 bg-base-900 dark:bg-base-50 dark:text-base-900 hover:bg-base-700 dark:hover:bg-base-200 rounded-md shadow-sm transition-colors"
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
        <div class="fixed inset-0 z-[100] flex items-center justify-center bg-base-900/50 backdrop-blur-sm p-4">
            <div class="bg-base-50 dark:bg-base-900 w-full max-w-md p-6 rounded-lg shadow-2xl border border-base-200 dark:border-base-800 animate-in zoom-in-95 duration-200">
                <h3 class="text-lg font-bold text-base-900 dark:text-base-50 mb-2">
                    "¿Eliminar archivo?"
                </h3>
                <p class="text-sm text-base-500 dark:text-base-400 mb-6">
                    "Estás a punto de borrar " <span class="font-mono text-xs">{path.clone()}</span>
                    ". Esta acción no se puede deshacer."
                </p>

                <div class="flex justify-end gap-3">
                    <button
                        on:click=move |_| on_cancel.run(())
                        class="px-4 py-2 text-sm font-medium text-base-600 dark:text-base-400 hover:bg-base-100 dark:hover:bg-base-800 rounded-md transition-colors"
                    >
                        "Cancelar"
                    </button>
                    <button
                        on:click=move |_| on_confirm.run(())
                        class="px-4 py-2 text-sm font-medium text-base-50 bg-brand-orange hover:bg-brand-orange/80 rounded-md shadow-sm transition-colors"
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
        <div class="fixed inset-0 z-[100] flex items-center justify-center bg-base-900/50 backdrop-blur-sm p-4">
            <div class="bg-base-50 dark:bg-base-900 w-full max-w-md p-6 rounded-lg shadow-2xl border border-base-200 dark:border-base-800 animate-in zoom-in-95 duration-200">
                <h3 class="text-lg font-bold text-base-900 dark:text-base-50 mb-2">
                    "Renombrar archivo"
                </h3>
                <p class="text-xs text-base-500 dark:text-base-400 mb-4 truncate">
                    "Ruta: " {path.clone()}
                </p>

                <input
                    type="text"
                    class="w-full px-3 py-2 bg-base-100 dark:bg-base-800 border border-base-200 dark:border-base-700 rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-brand-orange mb-6 text-base-900 dark:text-base-100"
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
                        class="px-4 py-2 text-sm font-medium text-base-600 dark:text-base-400 hover:bg-base-100 dark:hover:bg-base-800 rounded-md transition-colors"
                    >
                        "Cancelar"
                    </button>
                    <button
                        on:click=move |_| on_confirm.run(new_name.get())
                        class="px-4 py-2 text-sm font-medium text-base-50 bg-base-900 hover:bg-base-700 rounded-md shadow-sm transition-colors"
                    >
                        "Guardar cambios"
                    </button>
                </div>
            </div>
        </div>
    }
}
