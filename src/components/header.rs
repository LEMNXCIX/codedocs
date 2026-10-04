use leptos::prelude::*;

use crate::state::EditorState;

/// Filename, dirty indicator and save-state dot.
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
            <Dot state />
        </header>
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
