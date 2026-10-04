use leptos::prelude::*;

/// Close on Escape, for the lifetime of the calling component.
///
/// The three modals each registered their own `window` keydown listener and
/// never removed it, so opening the delete dialog three times left three live
/// listeners that all fired on Escape. Tying the listener to the component's
/// lifetime fixes that.
fn close_on_escape(on_cancel: impl Fn() + Copy + Send + Sync + 'static) {
    let listener =
        window_event_listener(leptos::ev::keydown, move |ev: leptos::ev::KeyboardEvent| {
            if ev.key() == "Escape" {
                on_cancel();
            }
        });
    on_cleanup(move || listener.remove());
}

/// Shell shared by the dialogs.
///
/// `confirm_kind` only selects the button colour; the markup was duplicated
/// three times before, which is how the delete dialog ended up with a
/// different button style from the other two.
#[component]
fn ModalShell(
    title: String,
    /// Rendered above the body when non-empty.
    subtitle: String,
    children: Children,
    confirm_label: String,
    confirm_kind: &'static str,
    confirm_disabled: RwSignal<bool>,
    on_confirm: Callback<()>,
    on_cancel: Callback<()>,
) -> impl IntoView {
    // Resolved once: `confirm_kind` is a `'static str` set at each call site,
    // so this cannot change while the dialog is open.
    let confirm_class = match confirm_kind {
        "danger" => "bg-red-600 hover:bg-red-700 text-base-50",
        "brand" => "bg-brand-orange hover:bg-brand-orange/80 text-base-900",
        _ => {
            "bg-base-900 hover:bg-base-700 text-base-50 \
              dark:bg-base-50 dark:text-base-900 dark:hover:bg-base-200"
        }
    };
    let base = "px-4 py-2 text-sm font-medium rounded-md shadow-sm transition-colors \
                disabled:opacity-50 disabled:cursor-not-allowed";
    let has_subtitle = !subtitle.is_empty();

    view! {
        <div class="fixed inset-0 z-[100] flex items-center justify-center \
                   bg-base-900/50 backdrop-blur-sm p-4">
            <div
                class="bg-base-50 dark:bg-base-900 w-full max-w-md p-6 rounded-lg shadow-2xl \
                       border border-base-200 dark:border-base-800 animate-in zoom-in-95 \
                       duration-200"
                role="dialog"
                aria-modal="true"
            >
                <h3 class="text-lg font-bold text-base-900 dark:text-base-50 mb-2">{title}</h3>
                <Show when=move || has_subtitle>
                    <p class="text-xs font-mono text-base-500 dark:text-base-400 mb-4 truncate">
                        {subtitle.clone()}
                    </p>
                </Show>
                <div class="mb-6">{children()}</div>
                <div class="flex justify-end gap-3">
                    <button
                        class="px-4 py-2 text-sm font-medium text-base-600 dark:text-base-400 \
                               hover:bg-base-100 dark:hover:bg-base-800 rounded-md transition-colors"
                        on:click=move |_| on_cancel.run(())
                    >
                        "Cancelar"
                    </button>
                    <button
                        class=format!("{base} {confirm_class}")
                        disabled=move || confirm_disabled.get()
                        on:click=move |_| on_confirm.run(())
                    >
                        {confirm_label}
                    </button>
                </div>
            </div>
        </div>
    }
}

#[component]
pub fn AlertModal(
    title: String,
    message: String,
    confirm_label: &'static str,
    on_confirm: Callback<()>,
    on_cancel: Callback<()>,
) -> impl IntoView {
    close_on_escape(move || on_cancel.run(()));

    view! {
        <ModalShell
            title
            subtitle=String::new()
            confirm_label=confirm_label.to_string()
            confirm_kind="default"
            confirm_disabled=RwSignal::new(false)
            on_confirm
            on_cancel
        >
            <p class="text-sm text-base-500 dark:text-base-400">{message}</p>
        </ModalShell>
    }
}

#[component]
pub fn DeleteConfirmModal(
    path: String,
    on_confirm: Callback<()>,
    on_cancel: Callback<()>,
) -> impl IntoView {
    close_on_escape(move || on_cancel.run(()));

    let subtitle = format!("Ruta: {path}");

    view! {
        <ModalShell
            title="¿Eliminar archivo?".to_string()
            subtitle=subtitle
            confirm_label="Eliminar permanentemente".to_string()
            confirm_kind="danger"
            confirm_disabled=RwSignal::new(false)
            on_confirm
            on_cancel
        >
            <p class="text-sm text-base-500 dark:text-base-400">
                "Vas a borrar " <span class="font-mono text-xs break-all">{path.clone()}</span>
                ". Si el archivo no está guardado en ningún control de versiones, \
                 no vas a poder recuperarlo."
            </p>
        </ModalShell>
    }
}

#[component]
pub fn RenameConfirmModal(
    path: String,
    on_confirm: Callback<String>,
    on_cancel: Callback<()>,
) -> impl IntoView {
    close_on_escape(move || on_cancel.run(()));

    let initial_name = crate::components::header::file_name(&path);
    let (new_name, set_new_name) = signal(initial_name.clone());
    // Guards against submitting an empty or unchanged name; the backend rejects
    // both, but failing here keeps the error out of the user's face.
    let can_confirm = RwSignal::new(false);

    let subtitle = format!("Ruta: {path}");

    view! {
        <ModalShell
            title="Renombrar archivo".to_string()
            subtitle=subtitle
            confirm_label="Guardar cambios".to_string()
            confirm_kind="default"
            confirm_disabled=can_confirm
            on_confirm=Callback::new(move |_| on_confirm.run(new_name.get().trim().to_string()))
            on_cancel
        >
            <input
                type="text"
                class="w-full px-3 py-2 bg-base-100 dark:bg-base-800 border \
                       border-base-200 dark:border-base-700 rounded-md text-sm \
                       focus:outline-none focus:ring-2 focus:ring-brand-orange \
                       text-base-900 dark:text-base-100"
                autofocus
                prop:value=move || new_name.get()
                on:input=move |ev| {
                    let value = event_target_value(&ev);
                    can_confirm.set(!value.trim().is_empty() && value.trim() != initial_name);
                    set_new_name.set(value);
                }
                on:keydown=move |ev| {
                    if ev.key() == "Enter" && can_confirm.get_untracked() {
                        on_confirm.run(new_name.get().trim().to_string());
                    }
                }
            />
            <p class="text-[11px] text-base-400 dark:text-base-500 mt-2">
                "La extensión debe ser .md y el nombre no puede incluir rutas."
            </p>
        </ModalShell>
    }
}
