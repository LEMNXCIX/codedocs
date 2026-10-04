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

#[cfg(test)]
mod tests {
    use super::*;

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
