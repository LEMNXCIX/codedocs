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
/// and the user is told the listing is partial.
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

    match tree {
        // The workspace check above already proved the root is a directory, so a
        // missing root here means the folder went away between the two calls,
        // and an empty result means there is nothing this editor can open.
        Err(TreeError::RootMissing { .. }) => Err("La carpeta ya no existe".to_string()),
        Err(err) => Err(err.to_string()),
        Ok(tree) if tree.entries.is_empty() => {
            Err("No se encontraron archivos Markdown en la carpeta seleccionada".to_string())
        }
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
#[tauri::command]
pub fn save_file(
    state: State<'_, AppState>,
    path_str: String,
    content: String,
) -> Result<(), String> {
    let workspace = state.current_workspace()?;
    let path = workspace
        .resolve_file(&path_str)
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
    write_atomically(&path, &content)
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
