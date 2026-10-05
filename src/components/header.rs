use leptos::prelude::*;

use crate::state::EditorState;
use crate::utils::env::is_tauri;

/// Filename, dirty indicator, export button and save-state dot.
#[component]
pub fn EditorHeader(state: EditorState) -> impl IntoView {
    view! {
        <header class="h-12 border-b border-base-200 dark:border-base-800 flex items-center \
                        justify-between px-4 bg-base-50 dark:bg-base-900/50 backdrop-blur-md z-10">
            <span class="text-xs font-mono text-base-400 dark:text-base-500 truncate">
                {move || match state.selected_file.get() {
                    Some(path) => file_name(&path),
                    None => "Sin archivo seleccionado".to_string(),
                }}
            </span>
            <div class="flex items-center gap-3 shrink-0">
                <ExportPdfButton state />
                <Dot state />
            </div>
        </header>
    }
}

/// "Exportar PDF", the visible half of `Ctrl+P`.
///
/// Rendered in the browser demo too, and disabled there: the export needs a
/// native dialog and a compiler, neither of which the web build has. Hiding it
/// instead would make the feature look absent, and the disabled button says
/// exactly why nothing happens.
///
/// Disabled with no file open for the backend's reason: the note's folder is
/// the compiler's root, and a document that has never been saved has none.
#[component]
fn ExportPdfButton(state: EditorState) -> impl IntoView {
    let on_click = move |_: leptos::ev::MouseEvent| crate::actions::export_pdf(state);

    view! {
        <button
            type="button"
            class="flex items-center gap-1.5 px-2 py-1 rounded-md text-[11px] font-medium \
                   text-base-500 dark:text-base-400 hover:text-base-800 dark:hover:text-base-200 \
                   hover:bg-base-200 dark:hover:bg-base-800 transition-colors \
                   disabled:opacity-40 disabled:cursor-not-allowed disabled:hover:bg-transparent \
                   disabled:hover:text-base-500 dark:disabled:hover:text-base-400"
            title="Exportar a PDF (Ctrl+P)"
            aria-label="Exportar a PDF"
            disabled=move || !is_tauri() || state.selected_file.get().is_none()
            on:click=on_click
        >
            <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24"
                fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"
                stroke-linejoin="round" class="flex-shrink-0">
                <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
                <polyline points="7 10 12 15 17 10"/>
                <line x1="12" y1="15" x2="12" y2="3"/>
            </svg>
            "PDF"
        </button>
    }
}

/// Unsaved-changes marker.
///
/// The header previously showed only the path, so there was no indication that
/// edits were pending — or that the last write had failed.
#[component]
fn Dot(state: EditorState) -> impl IntoView {
    view! {
        <Show when=move || {
            !matches!(state.save_state.get(), crate::state::SaveState::Idle)
        }>
            <span
                class="w-2 h-2 rounded-full shrink-0"
                class:bg-red-500=move || state.save_state.get().is_error()
                class:bg-brand-orange=move || {
                    !state.save_state.get().is_error()
                        && state.save_state.get() == crate::state::SaveState::Dirty
                }
                class:bg-base-300=move || {
                    !state.save_state.get().is_error()
                        && state.save_state.get() != crate::state::SaveState::Dirty
                }
                title=move || state.save_state.get().label()
            />
        </Show>
    }
}

/// Last path segment, for both separators.
pub fn file_name(path: &str) -> String {
    path.rsplit(['/', '\\'])
        .find(|segment| !segment.is_empty())
        .unwrap_or(path)
        .to_string()
}

/// The folder containing `path`, for both separators.
///
/// `None` when there is no folder to speak of: a bare name, or a root. Callers
/// use that to tell "the path names a folder we can work in" from "we would have
/// to invent one", instead of falling back to the file itself — which would make
/// the workspace a document.
pub fn parent_dir(path: &str) -> Option<String> {
    // Trailing separators are noise (`/a/b/` is the folder `/a/b`, same as
    // `std::path::Path` treats it), and they are what makes `/` collapse to an
    // empty string before the search below.
    let trimmed = path.trim_end_matches(['/', '\\']);
    let cut = trimmed.rfind(['/', '\\'])?;
    let parent = &trimmed[..cut];
    if parent.is_empty() || parent.ends_with(':') {
        // The file sits at a root (`/notas.md`, `C:\notas.md`): the parent *is*
        // that root, and stopping at the separator would leave `""` or `C:`,
        // neither of which is a folder anything can resolve.
        Some(trimmed[..=cut].to_string())
    } else {
        Some(parent.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_the_folder_of_a_unix_path() {
        assert_eq!(
            parent_dir("/home/user/notes/a.md").as_deref(),
            Some("/home/user/notes")
        );
    }

    #[test]
    fn takes_the_folder_of_a_windows_path() {
        assert_eq!(
            parent_dir(r"C:\Users\me\notes\a.md").as_deref(),
            Some(r"C:\Users\me\notes")
        );
    }

    #[test]
    fn a_bare_name_has_no_folder() {
        assert_eq!(parent_dir("a.md"), None);
    }

    #[test]
    fn trailing_separators_are_ignored() {
        assert_eq!(parent_dir("/a/b/").as_deref(), Some("/a"));
        assert_eq!(parent_dir("/a/b///").as_deref(), Some("/a"));
    }

    #[test]
    fn a_root_has_no_folder() {
        assert_eq!(parent_dir("/"), None);
        assert_eq!(parent_dir(r"C:\"), None);
    }

    #[test]
    fn a_file_at_the_root_keeps_the_root() {
        assert_eq!(parent_dir("/a.md").as_deref(), Some("/"));
        assert_eq!(parent_dir(r"C:\a.md").as_deref(), Some(r"C:\"));
    }

    #[test]
    fn extracts_unix_names() {
        assert_eq!(file_name("/home/user/notes/a.md"), "a.md");
    }

    #[test]
    fn extracts_windows_names() {
        assert_eq!(file_name(r"C:\Users\me\notes\a.md"), "a.md");
    }

    #[test]
    fn tolerates_trailing_separator() {
        assert_eq!(file_name("/a/b/"), "b");
    }

    #[test]
    fn falls_back_to_the_input() {
        assert_eq!(file_name(""), "");
    }
}
