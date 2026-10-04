//! Folder watching.
//!
//! The backend watches exactly the directories the sidebar shows, so it never
//! registers inotify watches for `.git` or `node_modules`. Two events matter
//! here:
//!
//! * `fs-change` — a markdown file changed on disk.
//! * `fs-new-dir` — a folder appeared, so the watch set has to be rebuilt.

use leptos::prelude::*;
use leptos::reactive::spawn_local;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;

use crate::state::EditorState;
use crate::utils::tauri_bridge;

/// Event emitted when markdown files change on disk.
const FS_CHANGE: &str = "fs-change";
/// Event emitted when a directory is created inside the workspace.
const FS_NEW_DIR: &str = "fs-new-dir";

/// Start watching the open workspace.
///
/// Installs the backend watcher and subscribes to its events. Both are released
/// when the owning component is cleaned up; the previous implementation leaked
/// both, so watchers accumulated and kept reloading the open file after it was
/// gone.
pub fn watch_workspace(state: EditorState) {
    let Some(path) = state.path.get_untracked() else {
        return;
    };
    if !crate::utils::env::is_tauri() {
        return;
    }

    // Registered here, synchronously, because `on_cleanup` needs a current owner
    // and a spawned future has none — calling it inside `spawn_local` silently
    // does nothing. The subscription ids only exist after the awaits below, so
    // the cleanup reads them from a signal instead of capturing them.
    let subscriptions: RwSignal<Vec<(String, i32)>> = RwSignal::new(Vec::new());
    on_cleanup(move || {
        for (event, id) in subscriptions.get_untracked() {
            tauri_bridge::unlisten(&event, id);
        }
        // Re-arming already replaces the backend watcher, so this only matters
        // on teardown; without it the OS keeps watching a folder the user has
        // navigated away from.
        spawn_local(async {
            let _ = tauri_bridge::stop_watching().await;
        });
    });

    spawn_local(async move {
        rearm(&path).await;
        subscribe(state, subscriptions).await;
    });
}

/// Ask the backend to (re)install its watcher for `path`.
///
/// Called on startup and again whenever a new folder appears, because the
/// backend only watches the directories that existed when it built its list.
pub async fn rearm(path: &str) {
    if let Err(err) = tauri_bridge::watch_folder(path).await {
        leptos::logging::error!("No se pudo observar la carpeta: {err}");
    }
}

/// Subscribe to the backend's watcher events, recording the ids so the caller's
/// cleanup can detach them.
async fn subscribe(state: EditorState, subscriptions: RwSignal<Vec<(String, i32)>>) {
    register(
        FS_CHANGE,
        Closure::<dyn Fn(wasm_bindgen::JsValue)>::new(move |event| {
            let changed = tauri_bridge::parse_fs_change(&event);
            if !changed.is_empty() {
                handle_fs_change(state, changed);
            }
        }),
        subscriptions,
    )
    .await;

    register(
        FS_NEW_DIR,
        Closure::<dyn Fn(wasm_bindgen::JsValue)>::new(move |_| handle_new_dir(state)),
        subscriptions,
    )
    .await;
}

/// Attach one event listener and record its id.
///
/// A `Closure<dyn Fn>` is neither `Send` nor `Sync`, so it cannot be moved into
/// `on_cleanup`. The Rust box is therefore leaked on purpose — one per opened
/// folder — while the *JS* listener, the part that keeps firing, is detached by
/// the caller's cleanup using the id recorded here.
async fn register(
    event: &str,
    closure: Closure<dyn Fn(wasm_bindgen::JsValue)>,
    subscriptions: RwSignal<Vec<(String, i32)>>,
) {
    match tauri_bridge::listen(event, closure.as_ref().unchecked_ref()).await {
        Some(id) => {
            subscriptions.update(|list| list.push((event.to_string(), id)));
            closure.forget();
        }
        None => leptos::logging::error!("No se pudo suscribir al evento '{event}'"),
    }
}

/// React to files changing on disk.
///
/// If the open file changed, its content is reloaded. The reload also updates
/// `last_saved`: without that, the autosave effect sees the buffer differ from
/// `last_saved`, writes the file straight back, and the resulting write
/// re-triggers the watcher — an unbounded write loop.
fn handle_fs_change(state: EditorState, changed: Vec<String>) {
    if let Some(current) = state.selected_file.get_untracked() {
        if changed.iter().any(|p| paths_match(p, &current)) {
            spawn_local(async move {
                match tauri_bridge::read_file(&current).await {
                    Ok(content) => {
                        // The user's unsaved buffer wins over the copy on disk.
                        if state.content.get_untracked() != state.last_saved.get_untracked() {
                            return;
                        }
                        state.content.set(content.clone());
                        state.last_saved.set(content);
                        state.save_state.set(crate::state::SaveState::Saved);
                    }
                    Err(err) => leptos::logging::error!("Error al recargar '{current}': {err}"),
                }
            });
        }
    }
    crate::actions::refresh_files(state);
}

/// A folder appeared: refresh the tree and rebuild the watch set.
///
/// The backend could not have watched this folder, because it only watches what
/// existed when it enumerated the tree.
fn handle_new_dir(state: EditorState) {
    let Some(path) = state.path.get_untracked() else {
        return;
    };
    spawn_local(async move {
        rearm(&path).await;
    });
    crate::actions::refresh_files(state);
}

/// Compare two paths tolerantly.
///
/// The backend emits OS-native paths, but separators can differ between what the
/// tree reports and what the watcher produces, so normalise before comparing.
fn paths_match(a: &str, b: &str) -> bool {
    a == b || normalize(a) == normalize(b)
}

fn normalize(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_paths_match() {
        assert!(paths_match("/a/b.md", "/a/b.md"));
    }

    #[test]
    fn separators_are_normalised() {
        assert!(paths_match("C:\\docs\\a.md", "C:/docs/a.md"));
        assert!(paths_match("C:\\docs\\a.md", "c:/docs/a.md"));
    }

    #[test]
    fn trailing_separators_are_ignored() {
        assert!(paths_match("/a/b/", "/a/b"));
    }

    #[test]
    fn different_paths_do_not_match() {
        assert!(!paths_match("/a/b.md", "/a/c.md"));
        // Guards against a prefix match: `/a/b` must not match `/a/bc`.
        assert!(!paths_match("/a/b", "/a/bc"));
    }

    #[test]
    fn watcher_ignores_the_same_directories_as_the_tree() {
        // The shared rule is what keeps the watcher off `node_modules`.
        for dir in ["node_modules", "target", ".git", "__pycache__", "Pods"] {
            assert!(
                codedocs_core::should_skip_dir(dir),
                "{dir} should be skipped by both the tree and the watcher"
            );
        }
        assert!(!codedocs_core::should_skip_dir("notes"));
    }
}
