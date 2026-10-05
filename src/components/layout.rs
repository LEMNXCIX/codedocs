use leptos::prelude::*;
use leptos::reactive::spawn_local;

use crate::actions::{self, AutosaveTimer};
use crate::components::editor::EditorPane;
use crate::components::header::EditorHeader;
use crate::components::modals::{AlertModal, DeleteConfirmModal, RenameConfirmModal};
use crate::components::sidebar::Sidebar;
use crate::components::status_bar::StatusBar;
use crate::state::{EditorState, SaveState};
use crate::utils::markdown::extract_headings;

/// Root layout: sidebar, editor pane, status bar, modals and toast.
///
/// This component owns *wiring* only. State lives in [`EditorState`], the
/// filesystem operations live in [`crate::actions`], the shortcut table in
/// [`crate::shortcuts`] and the file watcher in [`crate::watch`], so the
/// effects below stay short enough to read.
#[component]
pub fn Layout() -> impl IntoView {
    let state = EditorState::new();
    provide_context(state);

    let autosave_timer: RwSignal<Option<AutosaveTimer>> = RwSignal::new(None);
    let (file_to_delete, set_file_to_delete) = signal::<Option<String>>(None);
    let (file_to_rename, set_file_to_rename) = signal::<Option<String>>(None);
    let (show_clear_confirm, set_show_clear_confirm) = signal(false);

    install_sidebar_resize(state);
    seed_theme(state);

    // Refresh the outline whenever the buffer changes. Doing the markdown
    // parse in an effect rather than inline in `view!` keeps it off the
    // render path of every unrelated signal.
    Effect::new(move |_| {
        let content = state.content.get();
        state.headings.set(extract_headings(&content));
    });

    // Autosave. The `content != last_saved` guard is what breaks the loop where a
    // watcher-triggered reload would schedule another write of the same bytes.
    Effect::new(move |_| {
        let content = state.content.get();
        if content == state.last_saved.get() {
            return;
        }
        state.save_state.set(SaveState::Dirty);
        actions::schedule_autosave(state, autosave_timer);
    });

    let on_save = Callback::new(move |_| {
        spawn_local(async move {
            actions::save_now(state).await;
        });
    });

    crate::shortcuts::install(state, on_save);

    // Creating a file needs a folder to put it in, and asking for one is the
    // same prompt as "save a document that has no file yet" — so both go
    // through `ensure_workspace` instead of each deciding for itself.
    let on_create = Callback::new(move |_: ()| {
        spawn_local(async move {
            let Some(folder) = actions::ensure_workspace(state).await else {
                return;
            };
            actions::create_file(state, folder);
        });
    });

    view! {
        <div class="flex flex-col h-screen bg-base-50 dark:bg-base-900 text-base-900 \
                    dark:text-base-200 font-sans transition-all duration-300 overflow-hidden">
            <div
                class="flex flex-row flex-1 min-h-0"
                class:select-none=move || state.is_resizing_sidebar.get()
                class:cursor-col-resize=move || state.is_resizing_sidebar.get()
            >
                <Sidebar
                    state
                    on_delete=Callback::new(move |path| set_file_to_delete.set(Some(path)))
                    on_rename=Callback::new(move |path| set_file_to_rename.set(Some(path)))
                    create_new_file=on_create
                />

                <div
                    class="w-1 hover:w-1.5 bg-transparent hover:bg-brand-orange/40 \
                           cursor-col-resize transition-all z-50 flex-shrink-0"
                    on:mousedown=move |_| state.is_resizing_sidebar.set(true)
                />

                <main class="flex-1 flex flex-col min-w-0 bg-base-50 dark:bg-base-900 overflow-hidden">
                    <EditorHeader state />
                    <EditorPane state on_save />
                </main>
            </div>

            <StatusBar state on_save />

            {move || file_to_delete.get().map(|path| {
                view! {
                    <DeleteConfirmModal
                        path=path.clone()
                        on_confirm=Callback::new(move |_| {
                            actions::delete_file(state, path.clone());
                            set_file_to_delete.set(None);
                        })
                        on_cancel=Callback::new(move |_| set_file_to_delete.set(None))
                    />
                }
            })}

            {move || file_to_rename.get().map(|path| {
                view! {
                    <RenameConfirmModal
                        path=path.clone()
                        on_confirm=Callback::new(move |new_name| {
                            actions::rename_file(state, path.clone(), new_name);
                            set_file_to_rename.set(None);
                        })
                        on_cancel=Callback::new(move |_| set_file_to_rename.set(None))
                    />
                }
            })}

            {move || if show_clear_confirm.get() {
                view! {
                    <AlertModal
                        title="Limpiar editor".to_string()
                        message="¿Estás seguro de que quieres limpiar todo el contenido? \
                                Vas a tener que guardar para conservarlo."
                            .to_string()
                        confirm_label="Limpiar"
                        on_confirm=Callback::new(move |_| {
                            actions::clear_editor(state);
                            set_show_clear_confirm.set(false);
                        })
                        on_cancel=Callback::new(move |_| set_show_clear_confirm.set(false))
                    />
                }
                .into_any()
            } else {
                ().into_any()
            }}

            <Toast state />
        </div>
    }
}

/// Drag-to-resize the sidebar.
///
/// Both listeners are removed on cleanup; the previous code discarded the
/// handles, which left them attached for the lifetime of the webview.
fn install_sidebar_resize(state: EditorState) {
    let on_move =
        window_event_listener(leptos::ev::mousemove, move |ev: leptos::ev::MouseEvent| {
            if !state.is_resizing_sidebar.get_untracked() {
                return;
            }
            let width = ev.client_x() as f64;
            if (160.0..=480.0).contains(&width) {
                state.sidebar_width.set(width);
            }
        });
    on_cleanup(move || on_move.remove());

    let on_up = window_event_listener(leptos::ev::mouseup, move |_| {
        state.is_resizing_sidebar.set(false);
    });
    on_cleanup(move || on_up.remove());
}

/// Transient status message.
///
/// Rendered here rather than cleared by a timer: a timeout would leak a closure
/// per message, the same class of bug the autosave timer had. The message is
/// replaced by the next one, or dismissed by clicking it.
#[component]
fn Toast(state: EditorState) -> impl IntoView {
    view! {
        <Show when=move || state.toast.get().is_some()>
            <button
                class="fixed bottom-12 left-1/2 -translate-x-1/2 z-[110] px-4 py-2 rounded-md \
                       bg-brand-orange text-base-900 text-sm font-medium shadow-lg \
                       max-w-lg text-left"
                on:click=move |_| state.toast.set(None)
            >
                {move || state.toast.get().unwrap_or_default()}
            </button>
        </Show>
    }
}

/// Track the document colour scheme, seeding `is_dark` from the DOM.
///
/// Read from the `dark` class on `<html>`, which is the single source of truth
/// for both Tailwind's `dark:` variants and the editor theme. The previous code
/// sampled the class list inside an effect with no reactive dependency, so the
/// value was computed once and never updated when the user toggled the theme.
pub fn prefers_dark() -> bool {
    leptos::prelude::document()
        .document_element()
        .is_some_and(|el| el.class_list().contains("dark"))
}

/// Mirror the `dark` class into `state.is_dark`, once, at startup.
fn seed_theme(state: EditorState) {
    state.is_dark.set(prefers_dark());
}
