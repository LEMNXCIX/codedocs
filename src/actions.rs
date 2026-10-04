//! Project state and the operations that mutate it.
//!
//! Every filesystem interaction and every write to the editor buffer goes
//! through this module. Splitting it out of `Layout` is what makes the tricky
//! parts — the autosave debounce, the open-file race, the file watcher —
//! reviewable in isolation instead of being buried in a 467-line component.

use leptos::prelude::*;
use leptos::reactive::spawn_local;

use crate::state::{EditorState, SaveState, AUTOSAVE_DELAY_MS};
use crate::types::FileEntry;
use crate::utils::env::is_tauri;
use crate::utils::tauri_bridge;

/// Cancel a pending autosave timer, if any.
/// Handle for the pending autosave timer, held in a signal so the next
/// keystroke can cancel the previous timer.
pub type AutosaveTimer = TimeoutHandle;

/// Cancel a pending autosave timer, if any.
pub fn cancel_autosave(timer: Option<AutosaveTimer>) {
    if let Some(timer) = timer {
        timer.clear();
    }
}

/// Reload the file tree for the currently open folder.
///
/// A truncated listing is not a failure: whatever the walk found is applied as
/// it is. The walk budget is 100 000 entries and truncation is reported in the
/// IPC payload, but the user is not interrupted with a toast — for a folder
/// that large the partial listing is far more useful than the message, and the
/// banner turned every large folder into an error-looking screen.
pub fn refresh_files(state: EditorState) {
    let Some(path) = state.path.get_untracked() else {
        return;
    };
    spawn_local(async move {
        match tauri_bridge::list_markdown_files(&path).await {
            Ok(tree) => state.files.set(tree.entries),
            Err(err) => {
                leptos::logging::error!("No se pudo listar archivos: {err}");
                state.notify(err);
            }
        }
    });
}

/// Persist the current buffer to disk.
///
/// Returns the new `SaveState` so callers can decide whether to surface a
/// message; failures are never swallowed.
pub async fn save_now(state: EditorState) -> SaveState {
    let Some(path) = state.selected_file.get_untracked() else {
        return SaveState::Idle;
    };
    let content = state.content.get_untracked();
    state.save_state.set(SaveState::Saving);
    let outcome = write_buffer(&path, &content, state).await;
    state.save_state.set(outcome.clone());
    outcome
}

/// Write `content` to `path`, updating `last_saved` on success.
///
/// Split out from [`save_now`] so the autosave timer can persist an explicitly
/// captured buffer rather than re-reading a signal that may already have moved
/// on.
async fn write_buffer(path: &str, content: &str, state: EditorState) -> SaveState {
    if !is_tauri() {
        // The web demo has no filesystem; reflect that rather than pretending.
        return SaveState::Saved;
    }
    match tauri_bridge::save_file(path, content).await {
        Ok(()) => {
            state.last_saved.set(content.to_string());
            SaveState::Saved
        }
        Err(err) => {
            leptos::logging::error!("Error al guardar '{path}': {err}");
            state.save_state.set(SaveState::Failed(err.clone()));
            state.notify(format!("No se pudo guardar: {err}"));
            SaveState::Failed(err)
        }
    }
}

/// Schedule a debounced save, cancelling any timer already pending.
///
/// The content is captured *now* and written after the debounce, so a burst of
/// keystrokes results in exactly one write of the final text.
pub fn schedule_autosave(state: EditorState, timer: RwSignal<Option<AutosaveTimer>>) {
    let Some(path) = state.selected_file.get_untracked() else {
        return;
    };
    if !is_tauri() {
        return;
    }

    cancel_autosave(timer.get_untracked());

    // The buffer is captured now and written after the debounce, so a burst of
    // keystrokes produces one write of the final text.
    let content = state.content.get_untracked();
    let delay = std::time::Duration::from_millis(AUTOSAVE_DELAY_MS as u64);

    // `set_timeout_with_handle` owns the closure for us and hands back a handle
    // that cancels it. The previous implementation used
    // `Closure::once(..).forget()`, which leaked the captured document — a
    // full copy per keystroke — for the lifetime of the app.
    match set_timeout_with_handle(
        move || {
            spawn_local(async move {
                let outcome = write_buffer(&path, &content, state).await;
                // A keystroke may have landed during the write; if so the buffer
                // is dirty again and must not be reported as saved.
                if state.content.get_untracked() == content {
                    state.save_state.set(outcome);
                } else {
                    state.save_state.set(SaveState::Dirty);
                }
            });
        },
        delay,
    ) {
        Ok(handle) => timer.set(Some(handle)),
        Err(err) => leptos::logging::error!("No se pudo programar el auto-guardado: {err:?}"),
    }
}

/// Open a file into the editor.
///
/// Each call takes a token; a response whose token is stale is dropped. Without
/// this, opening two files in quick succession could leave the first file's
/// content in the buffer while the header showed the second file's name — and
/// autosave would then write one file's content over the other.
pub fn open_file(state: EditorState, full_path: String) {
    let token = state.open_token.get_untracked().wrapping_add(1);
    state.open_token.set(token);
    state.save_state.set(SaveState::Idle);

    spawn_local(async move {
        let content = if is_tauri() {
            match tauri_bridge::read_file(&full_path).await {
                Ok(content) => content,
                Err(err) => {
                    // A newer open request may have superseded this one.
                    if state.open_token.get_untracked() != token {
                        return;
                    }
                    leptos::logging::error!("Error al leer '{full_path}': {err}");
                    state.notify(format!("No se pudo abrir el archivo: {err}"));
                    state.selected_file.set(None);
                    state.content.set(String::new());
                    state.last_saved.set(String::new());
                    state.save_state.set(SaveState::Idle);
                    return;
                }
            }
        } else {
            demo_content(&full_path)
        };

        if state.open_token.get_untracked() != token {
            return;
        }

        state.selected_file.set(Some(full_path));
        state.content.set(content.clone());
        // Seeding `last_saved` is what stops the autosave effect from treating
        // a freshly opened file as a pending edit.
        state.last_saved.set(content);
        state.save_state.set(SaveState::Saved);
    });
}

/// Stand-in content for the browser demo build.
fn demo_content(full_path: &str) -> String {
    match full_path {
        "C:\\Demo\\Documents\\Bienvenido.md" => {
            "# 👋 Bienvenido a CodeDocs\n\nEsta es una **demo interactiva** en la web.\n\n\
             ### Características:\n- Edición rápida\n- Previsualización en tiempo real\n- \
             Soporte para plantillas"
                .to_string()
        }
        "C:\\Demo\\Documents\\Guía_Rápida.md" => {
            "# ⚡ Guía Rápida\n\n1. Selecciona un archivo.\n2. Edita su contenido.\n\
             3. Mira la preview a la derecha."
                .to_string()
        }
        _ => "# 📂 Archivo Demo\n\nContenido de ejemplo para la versión web.".to_string(),
    }
}

/// Pick a file name that does not collide with anything in the tree.
///
/// The previous implementation always used `Nuevo_Documento.md`, so the second
/// click failed with "El archivo ya existe" — an error that only reached the
/// console.
pub fn unique_file_name(existing: &[String]) -> String {
    const BASE: &str = "Nuevo_Documento";
    let taken = |candidate: &str| existing.iter().any(|n| n.eq_ignore_ascii_case(candidate));

    let first = format!("{BASE}.md");
    if !taken(&first) {
        return first;
    }
    let mut counter = 2;
    loop {
        let candidate = format!("{BASE}_{counter}.md");
        if !taken(&candidate) {
            return candidate;
        }
        counter += 1;
    }
}

/// Create a new markdown file in `folder` and open it.
pub fn create_file(state: EditorState, folder: String) {
    spawn_local(async move {
        let existing: Vec<String> = state
            .files
            .get_untracked()
            .iter()
            .flat_map(collect_names)
            .collect();
        let name = unique_file_name(&existing);

        match tauri_bridge::create_file(&folder, &name).await {
            Ok(path) => {
                refresh_files(state);
                open_file(state, path);
            }
            Err(err) => {
                leptos::logging::error!("Error al crear archivo: {err}");
                state.notify(format!("No se pudo crear el archivo: {err}"));
            }
        }
    });
}

#[cfg(test)]
mod unique_name_tests {
    use super::unique_file_name;

    #[test]
    fn first_name_when_tree_is_empty() {
        assert_eq!(unique_file_name(&[]), "Nuevo_Documento.md");
    }

    #[test]
    fn appends_a_counter_on_collision() {
        let existing = vec!["Nuevo_Documento.md".to_string()];
        assert_eq!(unique_file_name(&existing), "Nuevo_Documento_2.md");
    }

    #[test]
    fn skips_every_taken_name() {
        let existing = vec![
            "Nuevo_Documento.md".to_string(),
            "Nuevo_Documento_2.md".to_string(),
            "Nuevo_Documento_3.md".to_string(),
        ];
        assert_eq!(unique_file_name(&existing), "Nuevo_Documento_4.md");
    }

    #[test]
    fn comparison_is_case_insensitive() {
        let existing = vec!["nuevo_documento.MD".to_string()];
        assert_eq!(unique_file_name(&existing), "Nuevo_Documento_2.md");
    }

    #[test]
    fn unrelated_names_do_not_collide() {
        let existing = vec!["Otro.md".to_string()];
        assert_eq!(unique_file_name(&existing), "Nuevo_Documento.md");
    }
}

fn collect_names(entry: &FileEntry) -> Vec<String> {
    let mut names = vec![entry.name.clone()];
    names.extend(entry.children.iter().flat_map(collect_names));
    names
}

/// Delete a file after confirmation.
pub fn delete_file(state: EditorState, path: String) {
    spawn_local(async move {
        match tauri_bridge::delete_file(&path).await {
            Ok(()) => {
                if state.selected_file.get_untracked().as_deref() == Some(path.as_str()) {
                    state.selected_file.set(None);
                    state.content.set(String::new());
                    state.last_saved.set(String::new());
                    state.save_state.set(SaveState::Idle);
                }
                refresh_files(state);
            }
            Err(err) => {
                leptos::logging::error!("Error al eliminar '{path}': {err}");
                state.notify(format!("No se pudo eliminar: {err}"));
            }
        }
    });
}

/// Rename a file, keeping the editor consistent with the new name.
pub fn rename_file(state: EditorState, old_path: String, new_name: String) {
    spawn_local(async move {
        match tauri_bridge::rename_file(&old_path, &new_name).await {
            Ok(()) => {
                let was_open =
                    state.selected_file.get_untracked().as_deref() == Some(old_path.as_str());
                refresh_files(state);
                if was_open {
                    // Re-derive the new path rather than guessing at separators.
                    let renamed =
                        rename_in_tree(&state.files.get_untracked(), &old_path, &new_name);
                    match renamed {
                        Some(path) => open_file(state, path),
                        None => state.notify("El archivo se renombró; reabrilo desde la lista"),
                    }
                }
            }
            Err(err) => {
                leptos::logging::error!("Error al renombrar '{old_path}': {err}");
                state.notify(format!("No se pudo renombrar: {err}"));
            }
        }
    });
}

/// Find the new path of a renamed entry inside the current tree.
fn rename_in_tree(entries: &[FileEntry], old_path: &str, new_name: &str) -> Option<String> {
    for entry in entries {
        if entry.path == old_path {
            let parent = std::path::Path::new(&entry.path).parent()?;
            return Some(parent.join(new_name).to_string_lossy().into_owned());
        }
        if entry.is_dir {
            if let Some(found) = rename_in_tree(&entry.children, old_path, new_name) {
                return Some(found);
            }
        }
    }
    None
}

/// Clear the editor buffer without touching the file on disk.
pub fn clear_editor(state: EditorState) {
    state.content.set(String::new());
    // Compare-and-set: if the file was already empty there is nothing to save.
    if state.last_saved.get_untracked().is_empty() {
        state.save_state.set(SaveState::Idle);
    } else {
        state.save_state.set(SaveState::Dirty);
    }
}
