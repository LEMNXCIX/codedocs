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
    let Some(folder) = state.path.get_untracked() else {
        return;
    };
    spawn_local(async move {
        load_tree(state, &folder).await;
    });
}

/// Ask the backend for `folder`'s tree and show it.
///
/// Split out of [`refresh_files`] so a caller that needs the folder to *be* the
/// workspace before it can write can await it. `save_file` resolves the path it
/// is given against the open workspace, so the first save of a session has to
/// wait for this instead of racing it.
async fn load_tree(state: EditorState, folder: &str) {
    match tauri_bridge::list_markdown_files(folder).await {
        Ok(tree) => {
            state.files.set(tree.entries);
            // Now, and only now, does an empty tree mean "this folder has no
            // markdown". Before the response arrives it means "we do not know
            // yet", and the sidebar has to be able to tell those apart.
            state.tree_loaded.set(true);
        }
        Err(err) => {
            leptos::logging::error!("No se pudo listar archivos: {err}");
            state.notify(err);
        }
    }
}

/// The folder filesystem commands can run against, asking for one when there is
/// none.
///
/// Both "create a file" and "save a document that has no file yet" start here,
/// so the two cannot end up with different rules about when a folder is needed.
/// `None` means the user dismissed the picker, which is not a failure worth a
/// message — see [`is_cancelled`].
pub async fn ensure_workspace(state: EditorState) -> Option<String> {
    if let Some(folder) = state.path.get_untracked() {
        return Some(folder);
    }
    // The browser build has no picker and no disk behind it; its demo tree
    // stands in for a workspace, so there is nothing to ask for.
    if !is_tauri() {
        return None;
    }
    match tauri_bridge::open_project_folder().await {
        Ok(folder) => {
            adopt_workspace(state, folder.clone()).await;
            Some(folder)
        }
        Err(err) => {
            report_picker_failure(&state, "abrir la carpeta", &err);
            None
        }
    }
}

/// Make `folder` the workspace: the sidebar lists it and the backend resolves
/// paths against it.
///
/// Awaited rather than fired off, because a write issued in the same breath as
/// the folder switch would be resolved against the *previous* workspace — or
/// against none at all, which is what made the first save of a session fail with
/// "no hay ninguna carpeta abierta".
async fn adopt_workspace(state: EditorState, folder: String) {
    state.path.set(Some(folder.clone()));
    // The tree in the sidebar belongs to the *previous* folder from here on, so
    // it is not an answer about this one until `load_tree` says so.
    state.tree_loaded.set(false);
    load_tree(state, &folder).await;
}

/// Whether a native dialog was dismissed rather than failing.
///
/// The backend reports a closed picker as an error string with fixed wording
/// (`"Usuario cancelo la accion"`), because a command cannot return "nothing"
/// through Tauri. Matching on the words is what keeps closing a dialog from
/// being announced to the user as a red failure.
pub fn is_cancelled(err: &str) -> bool {
    err.contains("cancelo") || err.contains("cancel")
}

/// Report a picker failure, minus the "the user closed it" case.
fn report_picker_failure(state: &EditorState, action: &str, err: &str) {
    if is_cancelled(err) {
        return;
    }
    leptos::logging::error!("No se pudo {action}: {err}");
    state.notify(format!("No se pudo {action}: {err}"));
}

/// Persist the current buffer to disk.
///
/// Returns the new `SaveState` so callers can decide whether to surface a
/// message; failures are never swallowed.
pub async fn save_now(state: EditorState) -> SaveState {
    // A save already in flight owns the outcome. `Mod-s` has two independent
    // bindings — the global shortcut listener and the CodeMirror keymap — so one
    // Ctrl+S reaches this twice; without the guard the path below would ask for
    // a location twice, one native dialog per invocation.
    if state.save_state.get_untracked() == SaveState::Saving {
        return SaveState::Saving;
    }

    let Some(path) = state.selected_file.get_untracked() else {
        return save_as_new_file(state).await;
    };
    let content = state.content.get_untracked();
    state.save_state.set(SaveState::Saving);
    let outcome = write_buffer(&path, &content, state).await;
    state.save_state.set(outcome.clone());
    outcome
}

/// Write a buffer that has no file yet, asking the user where it should live.
///
/// The old behaviour returned [`SaveState::Idle`] and did nothing: opening the
/// app without a folder and pressing `Ctrl+S` looked like a broken shortcut.
/// Now the file's folder *becomes* the workspace, which is what makes the write
/// legal in the first place — the backend confines every path to the open
/// workspace, and there was none.
///
/// Deliberately not reachable from [`schedule_autosave`]: typing must never pop
/// a native dialog. The text waits in the buffer until the save is asked for.
async fn save_as_new_file(state: EditorState) -> SaveState {
    if !is_tauri() {
        // Same reason [`write_buffer`] bails here: the browser demo has no
        // filesystem to write to and no dialog to ask with.
        return SaveState::Idle;
    }

    // Claimed before the picker opens, not after: while the dialog is up the
    // buffer is still pending, and `save_now` uses this state to decide whether
    // another save is already running.
    let pending = state.save_state.get_untracked();
    state.save_state.set(SaveState::Saving);

    let route = match tauri_bridge::save_file_as().await {
        Ok(route) => route,
        Err(err) => {
            report_picker_failure(&state, "guardar el archivo", &err);
            return abandon(state, &pending);
        }
    };

    // No folder to resolve the path against yet: the one the user just picked is
    // installed first, then the write goes through the same path as any other
    // save.
    //
    // The native picker always hands back an absolute path, so this is a guard
    // rather than a branch anyone should see: writing anyway would fail with the
    // backend's "no hay ninguna carpeta abierta", which says nothing about the
    // path the user actually chose.
    let Some(folder) = crate::components::header::parent_dir(&route) else {
        leptos::logging::error!("La ruta elegida no tiene carpeta: '{route}'");
        state.notify("Esa ubicación no se puede usar: falta la carpeta que la contiene");
        return abandon(state, &pending);
    };
    adopt_workspace(state, folder).await;

    let content = state.content.get_untracked();
    let outcome = write_buffer(&route, &content, state).await;
    if outcome == SaveState::Saved {
        // Only now does the document have a name. Setting it before the write
        // would let a failed write leave the header pointing at a file that does
        // not exist, and the autosave would then retry forever.
        state.selected_file.set(Some(route));
        // The tree was loaded before the file existed, so the sidebar has to be
        // asked again for the document to show up in it.
        refresh_files(state);
    }
    // On failure `selected_file` is still `None`, so the next `Ctrl+S` asks for a
    // location again instead of retrying a path that just failed, and the
    // `Failed` state `write_buffer` reported is what stays on screen.
    state.save_state.set(outcome.clone());
    outcome
}

/// End a save that wrote nothing, putting the buffer back in the state it was in.
///
/// Restoring `pending` rather than picking a fresh state is what keeps cancelling
/// the dialog from reading as "saved" or as "failed": the text is still only in
/// memory, and the header dot has to keep saying so.
fn abandon(state: EditorState, pending: &SaveState) -> SaveState {
    state.save_state.set(pending.clone());
    pending.clone()
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
///
/// Returns without scheduling when there is no file. That is intentional and is
/// the reason this does not go through [`save_as_new_file`]: autosave must never
/// open a native dialog, or typing would make a picker pop up mid-sentence. The
/// text stays in the buffer until the save is asked for with `Ctrl+S`.
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
             ### Características:\n- Edición en vivo con formato aplicado\n- \
             `Ctrl+/` muestra la fuente Markdown\n- \
             Soporte para plantillas"
                .to_string()
        }
        "C:\\Demo\\Documents\\Guía_Rápida.md" => {
            "# ⚡ Guía Rápida\n\n1. Selecciona un archivo.\n2. Edita su contenido.\n\
             3. El formato se aplica mientras escribís; `Ctrl+/` muestra la fuente."
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

/// Export the open document as a PDF.
///
/// Needs a file: the backend compiles inside the note's own folder, so that a
/// relative image in the note resolves, and there is no folder for a document
/// that has never been saved. `ensure_workspace` is not called for it — asking
/// for a folder to *export* would be a surprise; a document with no file has
/// nothing to export yet, and `Ctrl+S` is the answer to that.
///
/// A dismiss of the save dialog is not an error worth a toast, for the same
/// reason it is not one for `save_file_as`: the backend reports it with the
/// fixed wording [`is_cancelled`] matches on.
pub fn export_pdf(state: EditorState) {
    spawn_local(async move {
        let Some(path) = state.selected_file.get_untracked() else {
            state.notify("Guardá el documento antes de exportarlo a PDF");
            return;
        };
        let content = state.content.get_untracked();
        match tauri_bridge::export_pdf(&path, &content).await {
            Ok(written) => state.notify(format!("PDF guardado en {written}")),
            Err(err) => report_picker_failure(&state, "exportar a PDF", &err),
        }
    });
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
