//! The entire filesystem surface of the app.
//!
//! Every command that touches disk resolves its path argument through the open
//! [`Workspace`] first. The frontend is JavaScript running in a webview that
//! renders untrusted markdown, so a command that trusts its path argument is
//! an arbitrary-file-read/write/delete primitive waiting for an XSS.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use codedocs_core::{
    build_tree, is_markdown_path, should_skip_dir, FileEntry, FileTree, TreeError, WalkLimits,
    Workspace,
};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::DialogExt;

use crate::state::AppState;

/// Largest document the editor will open or write.
///
/// Pulling a multi-gigabyte file into a webview wedges it for seconds and can
/// take the whole app down, so oversized files are refused up front.
const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;

/// Body of a freshly created note.
const NEW_FILE_TEMPLATE: &str = "# Nuevo archivo\n";

/// Ask the user for a project folder.
///
/// `async` on purpose: Tauri runs synchronous commands on the main thread,
/// where the native folder picker would freeze the webview for as long as it
/// stays open. The frontend checks for this exact wording to tell a cancelled
/// dialog apart from a real failure, so it must not be reworded.
#[tauri::command]
pub async fn open_project_folder(app: AppHandle) -> Result<String, String> {
    match app.dialog().file().blocking_pick_folder() {
        Some(folder) => Ok(folder.to_string()),
        None => Err("Usuario cancelo la accion".to_string()),
    }
}

/// Ask the user where to write a document that has no file yet.
///
/// Modelled on [`open_project_folder`] because it is the same interaction: an
/// `async` command, so the native picker does not run on the main thread and
/// freeze the webview, and the same cancellation wording, which the frontend
/// matches to tell a dismissed dialog apart from a real failure. Changing that
/// string turns every cancelled save into a red error toast.
#[tauri::command]
pub async fn save_file_as(app: AppHandle) -> Result<String, String> {
    match app
        .dialog()
        .file()
        .add_filter("Markdown", &["md", "markdown"])
        .set_file_name("untitled.md")
        .blocking_save_file()
    {
        Some(path) => Ok(path.to_string()),
        None => Err("Usuario cancelo la accion".to_string()),
    }
}

/// Ask where the PDF should go, export the open document there, and return the
/// path written.
///
/// `async` for the same reason as [`save_file_as`]: a synchronous command runs
/// on the main thread, where the native picker would freeze the webview for as
/// long as it stays open. The cancellation wording is that command's, verbatim:
/// the frontend matches those words to tell a dismissed dialog from a failure,
/// so it must not be reworded.
///
/// The destination is deliberately *not* resolved against the workspace: the
/// user pointed a save dialog at it, and a PDF in their Downloads folder is the
/// point of the feature. The note, on the other hand, goes through the guard
/// like every other file this app touches. See `export::export_to_pdf`.
#[tauri::command]
pub async fn export_pdf(
    app: AppHandle,
    state: State<'_, AppState>,
    path_str: String,
    content: String,
) -> Result<String, String> {
    let workspace = state.current_workspace()?;
    let note = workspace
        .resolve_file(&path_str)
        .map_err(|e| e.to_string())?;

    let chosen = app
        .dialog()
        .file()
        .add_filter("PDF", &["pdf"])
        .set_file_name(crate::export::suggested_name(&note))
        .blocking_save_file()
        .ok_or_else(|| "Usuario cancelo la accion".to_string())?;
    let destination = chosen
        .into_path()
        .map_err(|_| "Esa ubicacion no se puede usar".to_string())?;

    crate::export::export_to_pdf(
        &workspace,
        &path_str,
        &content,
        destination.to_string_lossy().as_ref(),
    )
    .await
}

/// Walk `folder_path` and return its markdown tree, opening it as the workspace
/// if it is not open yet.
///
/// The walk is the one filesystem operation that can touch thousands of inodes,
/// which is what made a large folder appear to hang: it runs on the blocking
/// pool instead of the async runtime, and [`WalkLimits::interactive`] caps both
/// the entry count and the nesting depth so an enormous tree reports back
/// instead of running until the UI stops responding.
///
/// Exhausting the budget is **not** an error. It returns the tree found so far
/// with `truncated` set, so a folder with a hundred thousand entries still opens
/// and the user is told the listing is partial. Neither is a folder with nothing
/// in it: that comes back as an empty tree, which is a state and not a failure
/// (see [`tree_outcome`]).
#[tauri::command]
pub async fn list_markdown_files(
    state: State<'_, AppState>,
    folder_path: String,
) -> Result<FileTree, String> {
    let workspace = state.open_workspace(Path::new(&folder_path))?;
    let root = workspace.root().to_path_buf();

    let tree =
        tauri::async_runtime::spawn_blocking(move || build_tree(&root, WalkLimits::interactive()))
            .await
            .map_err(|e| format!("No se pudo terminar de analizar la carpeta: {e}"))?;

    tree_outcome(tree)
}

/// What a finished walk means for the caller.
///
/// Split out of [`list_markdown_files`] so the one decision that was wrong —
/// treating "no markdown here" as a failure — can be asserted on the host
/// instead of only being observable by picking a folder in the native dialog.
///
/// The distinction: the root is already known to be a directory (the workspace
/// check refuses anything else), so a missing root here means the folder went
/// away mid-walk, and an unreadable one is a real I/O failure. An **empty** walk
/// is neither. The folder is fine, it just has nothing in it this editor can open
/// yet, so the tree goes back as it is and the frontend shows an empty state.
/// Returning an error here made the first save into a fresh folder — exactly the
/// flow that creates those folders — show a red toast a second before the save.
fn tree_outcome(walk: Result<FileTree, TreeError>) -> Result<FileTree, String> {
    match walk {
        Err(TreeError::RootMissing { .. }) => Err("La carpeta ya no existe".to_string()),
        Err(err) => Err(err.to_string()),
        Ok(tree) => Ok(tree),
    }
}

/// Read a markdown document from inside the workspace.
#[tauri::command]
pub fn read_file(state: State<'_, AppState>, path_str: String) -> Result<String, String> {
    let workspace = state.current_workspace()?;
    let path = workspace
        .resolve_file(&path_str)
        .map_err(|e| e.to_string())?;

    let size = fs::metadata(&path)
        .map_err(|e| format!("Error al leer el archivo: {e}"))?
        .len();
    if size > MAX_FILE_BYTES {
        return Err(format!(
            "El archivo es demasiado grande para abrirlo (máximo {} MB)",
            MAX_FILE_BYTES / (1024 * 1024)
        ));
    }

    fs::read_to_string(&path).map_err(|e| format!("Error al leer el archivo: {e}"))
}

/// Persist a markdown document inside the workspace.
///
/// The target does not have to exist yet: `Ctrl+S` on a document with no file
/// hands over a location the user just picked in the native dialog, and that
/// path is not on disk. See [`Workspace::resolve_write_target`], which is what
/// makes both cases legal without letting the write escape the workspace.
#[tauri::command]
pub fn save_file(
    state: State<'_, AppState>,
    path_str: String,
    content: String,
) -> Result<(), String> {
    let workspace = state.current_workspace()?;
    save_document(&workspace, &path_str, &content, &state)
}

/// The body of [`save_file`], minus Tauri plumbing.
///
/// Split out so the guard and the write can be tested against a real workspace
/// on the host, without a `State` and without a webview — which is the only way
/// to prove a save of a *new* file actually lands on disk and a save outside the
/// workspace actually fails. Neither is observable from the browser checks,
/// whose Tauri stub writes to disk without ever going through this guard.
fn save_document(
    workspace: &Workspace,
    path_str: &str,
    content: &str,
    state: &AppState,
) -> Result<(), String> {
    let path = workspace
        .resolve_write_target(path_str)
        .map_err(|e| e.to_string())?;

    if content.len() > MAX_FILE_BYTES as usize {
        return Err(format!(
            "El documento es demasiado grande para guardarlo (máximo {} MB)",
            MAX_FILE_BYTES / (1024 * 1024)
        ));
    }

    // Registered before the write, not after: the watcher thread can deliver
    // the event while `write_atomically` is still returning, and an unrecorded
    // write is exactly what starts the save/reload/autosave loop.
    state.note_write(&path);
    write_atomically(&path, content)
}

/// Delete a markdown document from inside the workspace.
#[tauri::command]
pub fn delete_file(state: State<'_, AppState>, path_str: String) -> Result<(), String> {
    let workspace = state.current_workspace()?;
    let path = workspace
        .resolve_file(&path_str)
        .map_err(|e| e.to_string())?;

    fs::remove_file(&path).map_err(|e| format!("Error al eliminar el archivo: {e}"))
}

/// Rename a markdown document, keeping it in its current folder.
///
/// `new_name` is validated by `Workspace::resolve_rename`: no separators, no
/// `..`, and it must end in `.md`, so a rename cannot be used to write outside
/// the workspace the way an unchecked `parent().join(name)` can.
#[tauri::command(rename_all = "camelCase")]
pub fn rename_file(
    state: State<'_, AppState>,
    old_path: String,
    new_name: String,
) -> Result<(), String> {
    let workspace = state.current_workspace()?;
    let (source, target) = workspace
        .resolve_rename(&old_path, &new_name)
        .map_err(|e| e.to_string())?;

    // `rename` silently clobbers on Unix, so an existing file is refused here
    // rather than left for the user to discover after the fact.
    if target.exists() {
        return Err(format!("Ya existe un archivo llamado '{new_name}'"));
    }

    fs::rename(&source, &target).map_err(|e| format!("Error al renombrar el archivo: {e}"))
}

/// Create an empty markdown document inside the workspace and return its path.
#[tauri::command]
pub fn create_file(
    state: State<'_, AppState>,
    folder_path: String,
    name: String,
) -> Result<String, String> {
    let workspace = state.current_workspace()?;
    let path = workspace
        .resolve_new_file(&folder_path, &name)
        .map_err(|e| e.to_string())?;

    if path.exists() {
        return Err("El archivo ya existe".to_string());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Error al crear la carpeta: {e}"))?;
    }

    // Recorded like any other write: the frontend opens the new file straight
    // away, and an unfiltered event for it would race that open.
    state.note_write(&path);
    write_atomically(&path, NEW_FILE_TEMPLATE)?;
    Ok(path.to_string_lossy().into_owned())
}

/// Watch the open workspace and emit `fs-change` for markdown edits.
///
/// Watches are registered **per directory, non-recursively**, over exactly the
/// directories [`watched_directories`] returns. A single recursive watch on the
/// project root registers an inotify watch on *every* subdirectory — including
/// `.git` and `node_modules`, which the tree walk deliberately skips — so
/// opening a large repository spent seconds and thousands of file descriptors
/// watching trees the app never displays. That was a second, independent cause
/// of the "the window freezes when I open a folder" symptom.
///
/// Directories created after startup are handled by `fs-new-dir`: the frontend
/// re-arms on that event rather than the watcher mutating itself from its own
/// event callback, which would mean locking the watcher from the notify thread.
#[tauri::command]
pub fn watch_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    folder_path: String,
) -> Result<(), String> {
    let workspace = state.open_workspace(Path::new(&folder_path))?;
    // `open_workspace` canonicalised the folder, so the watcher and the paths
    // in the events it delivers agree with what the workspace resolves to.
    let root = workspace.root().to_path_buf();
    let writes = state.write_log();
    let directories = watched_directories(&root)?;

    // Drop the previous watcher *before* building the replacement. Assigning
    // the new value straight over the old one would leave the old watcher
    // running until the assignment, and its queued events would still trigger
    // a full reload plus tree refresh for the folder we just left.
    state.clear_watcher()?;

    let mut watcher = RecommendedWatcher::new(
        move |result: notify::Result<notify::Event>| {
            let Ok(event) = result else { return };
            if !matches!(
                event.kind,
                EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
            ) {
                return;
            }

            // A new folder is only interesting if it holds markdown, which is
            // what the tree walk checks. Asking the frontend to re-arm keeps the
            // decision in one place instead of duplicating the skip rules here.
            let new_dirs: Vec<String> = event
                .paths
                .iter()
                .filter(|path| path.is_dir())
                .filter(|path| !contains_skipped_dir(path))
                .map(|path| path.to_string_lossy().into_owned())
                .collect();
            if !new_dirs.is_empty() {
                let _ = app.emit("fs-new-dir", &new_dirs);
            }

            let paths: Vec<String> = event
                .paths
                .iter()
                .filter(|path| is_markdown_path(path))
                .filter(|path| !writes.is_recent(path))
                .map(|path| path.to_string_lossy().into_owned())
                .collect();
            if paths.is_empty() {
                return;
            }
            // A failed emit means the webview is already gone; nothing to retry.
            let _ = app.emit("fs-change", &paths);
        },
        notify::Config::default(),
    )
    .map_err(|e| format!("Error al crear el watcher: {e}"))?;

    for dir in &directories {
        watcher
            .watch(dir, RecursiveMode::NonRecursive)
            .map_err(|e| format!("Error al observar '{}': {e}", dir.display()))?;
    }

    state.set_watcher(watcher)
}

/// Directories worth watching: the project root plus every folder that appears
/// in the markdown tree.
///
/// Derived from the same bounded walk that builds the sidebar, so the watcher and
/// the file tree always agree and the cost stays inside [`WalkLimits`]. A walk
/// that fails is not fatal: the root is always watched, so top-level changes are
/// still noticed.
fn watched_directories(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut directories = vec![root.to_path_buf()];
    if let Ok(tree) = build_tree(root, WalkLimits::interactive()) {
        collect_directories(&tree.entries, &mut directories);
    }
    Ok(directories)
}

fn collect_directories(entries: &[FileEntry], out: &mut Vec<PathBuf>) {
    for entry in entries {
        if entry.is_dir {
            out.push(PathBuf::from(&entry.path));
            collect_directories(&entry.children, out);
        }
    }
}

/// Whether any component of `path` is a directory the tree walk skips.
///
/// Delegates to the shared rule so the watcher can never disagree with the
/// sidebar about what exists.
fn contains_skipped_dir(path: &Path) -> bool {
    path.components()
        .any(|c| should_skip_dir(&c.as_os_str().to_string_lossy()))
}

/// Stop watching, if anything is being watched.
#[tauri::command]
pub fn stop_watching(state: State<'_, AppState>) -> Result<(), String> {
    state.clear_watcher()
}

/// Write `content` through a scratch file in the target directory and rename it
/// into place.
///
/// A plain `fs::write` truncates first, so a crash or a full disk mid-write
/// leaves the user with a destroyed document. The scratch file has to live in
/// the same directory as the target or the rename could cross filesystems and
/// stop being atomic, and it deliberately does not end in `.md` so the watcher
/// filter ignores it.
fn write_atomically(path: &Path, content: &str) -> Result<(), String> {
    let directory = path
        .parent()
        .ok_or_else(|| format!("Ruta inválida: '{}'", path.display()))?;
    let scratch = directory.join(format!(
        ".{}.codedocs-tmp",
        path.file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
    ));

    let written = (|| -> std::io::Result<()> {
        let mut file = fs::File::create(&scratch)?;
        file.write_all(content.as_bytes())?;
        // Flush before the rename: otherwise the file is visible at its final
        // path while its blocks are still unwritten.
        file.sync_all()
    })();

    if let Err(e) = written {
        let _ = fs::remove_file(&scratch);
        return Err(format!("Error al guardar el archivo: {e}"));
    }

    if let Err(e) = fs::rename(&scratch, path) {
        let _ = fs::remove_file(&scratch);
        return Err(format!("Error al guardar el archivo: {e}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh directory, canonicalised the way `Workspace::open` needs.
    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "codedocs-cmd-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.canonicalize().unwrap()
    }

    /// Saving a document with no file: the reported bug, end to end through the
    /// real guard and the real atomic write.
    ///
    /// The browser checks cannot catch this — their Tauri stub writes to disk
    /// without ever calling the guard — so this is the only place the claim
    /// "the guard now accepts a path that is not there yet" is actually tested.
    #[test]
    fn save_creates_a_file_that_did_not_exist() {
        let dir = scratch("newfile");
        let state = AppState::default();
        let ws = state.open_workspace(&dir).unwrap();
        let target = dir.join("sin-archivo.md");
        assert!(!target.exists());

        save_document(&ws, target.to_str().unwrap(), "# hola\n", &state)
            .expect("saving a new file should succeed");

        assert_eq!(fs::read_to_string(&target).unwrap(), "# hola\n");
        // The scratch file must not survive, or the watcher would see it.
        assert!(!dir.join(".sin-archivo.md.codedocs-tmp").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    /// The same save again, which now takes the existing-file branch. Every
    /// autosave after the first `Ctrl+S` lands here.
    #[test]
    fn saving_the_same_new_file_again_works() {
        let dir = scratch("twice");
        let state = AppState::default();
        let ws = state.open_workspace(&dir).unwrap();
        let raw = dir.join("notas.md").to_str().unwrap().to_string();

        save_document(&ws, &raw, "primera\n", &state).unwrap();
        save_document(&ws, &raw, "segunda\n", &state).unwrap();

        assert_eq!(
            fs::read_to_string(dir.join("notas.md")).unwrap(),
            "segunda\n"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// The defence that matters most: nothing outside the open workspace gets
    /// written, and the refusal is the guard's, not a failed write.
    #[test]
    fn save_refuses_a_path_outside_the_workspace() {
        let outside = scratch("outside");
        let dir = scratch("inside");
        let state = AppState::default();
        let ws = state.open_workspace(&dir).unwrap();
        let target = outside.join("plantado.md");

        let err = save_document(&ws, target.to_str().unwrap(), "# fuera\n", &state)
            .expect_err("a write outside the workspace must be refused");

        assert!(
            err.contains("fuera de la carpeta del proyecto"),
            "unexpected error: {err}"
        );
        assert!(!target.exists(), "nothing may be written outside");
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&outside);
    }

    /// The write is refused *before* any bytes move, so there is nothing to undo
    /// afterwards either.
    #[cfg(unix)]
    #[test]
    fn save_refuses_a_symlinked_parent_that_leaves_the_workspace() {
        let outside = scratch("parenttarget");
        let dir = scratch("parentroot");
        let state = AppState::default();
        let ws = state.open_workspace(&dir).unwrap();
        std::os::unix::fs::symlink(&outside, dir.join("escape")).unwrap();
        let target = dir.join("escape/plantado.md");

        assert!(save_document(&ws, target.to_str().unwrap(), "# fuera\n", &state).is_err());
        assert!(
            !outside.join("plantado.md").exists(),
            "the write followed a symlinked parent out of the workspace"
        );
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&outside);
    }

    /// An existing symlink to a file outside keeps being refused, even though
    /// the write path no longer goes through `resolve_file` unconditionally.
    #[cfg(unix)]
    #[test]
    fn save_refuses_an_existing_symlink_pointing_outside() {
        let outside = scratch("linktarget");
        let secret = outside.join("secret.md");
        fs::write(&secret, "top secret").unwrap();
        let dir = scratch("linkroot");
        std::os::unix::fs::symlink(&secret, dir.join("link.md")).unwrap();
        let state = AppState::default();
        let ws = state.open_workspace(&dir).unwrap();

        assert!(save_document(
            &ws,
            dir.join("link.md").to_str().unwrap(),
            "# pisado\n",
            &state
        )
        .is_err());
        assert_eq!(
            fs::read_to_string(&secret).unwrap(),
            "top secret",
            "the file the symlink pointed at was overwritten"
        );
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&outside);
    }

    /// An oversized document is refused without touching the target, new or not.
    #[test]
    fn save_refuses_an_oversized_document_without_creating_the_file() {
        let dir = scratch("toobig");
        let state = AppState::default();
        let ws = state.open_workspace(&dir).unwrap();
        let target = dir.join("enorme.md");
        let huge = "x".repeat(MAX_FILE_BYTES as usize + 1);

        let err = save_document(&ws, target.to_str().unwrap(), &huge, &state)
            .expect_err("an oversized document must be refused");
        assert!(err.contains("demasiado grande"), "unexpected: {err}");
        assert!(!target.exists(), "a refused save must not create the file");
        let _ = fs::remove_dir_all(&dir);
    }

    // ---- Bug 2: a folder with no markdown is a state, not a failure ----

    #[test]
    fn an_empty_folder_is_reported_as_an_empty_tree() {
        let dir = scratch("emptytree");
        let walk = build_tree(&dir, WalkLimits::interactive());

        let tree = tree_outcome(walk).expect("an empty folder is not an error");
        assert!(tree.entries.is_empty());
        assert!(tree.file_count() == 0);
        assert!(tree.truncated.is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    /// A folder whose markdown lives in a subdirectory still comes back non-empty,
    /// so the empty case cannot be reached by accident.
    #[test]
    fn a_folder_with_markdown_still_reports_its_tree() {
        let dir = scratch("fulltree");
        fs::create_dir_all(dir.join("notas")).unwrap();
        fs::write(dir.join("notas/una.md"), "# hola").unwrap();
        fs::write(dir.join("imagen.png"), "no cuenta").unwrap();

        let tree = tree_outcome(build_tree(&dir, WalkLimits::interactive())).unwrap();
        assert_eq!(tree.file_count(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    /// What is still an error: the folder is gone. Reported as such, not as an
    /// empty tree, so the frontend can tell the two apart.
    #[test]
    fn a_missing_folder_is_still_an_error() {
        let dir = scratch("gonetree");
        let gone = dir.join("no-existe");
        let err = tree_outcome(build_tree(&gone, WalkLimits::interactive()))
            .expect_err("a folder that is gone is an error");
        assert!(err.contains("ya no existe"), "unexpected: {err}");
        let _ = fs::remove_dir_all(&dir);
    }
}
