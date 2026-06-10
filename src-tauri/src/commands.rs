use std::{fs, path::Path, sync::Mutex};

use tauri_plugin_dialog::DialogExt;

use notify::{RecommendedWatcher, RecursiveMode, Watcher, Event, EventKind};
use tauri::Emitter;

static WATCHER: Mutex<Option<RecommendedWatcher>> = Mutex::new(None);

#[tauri::command]
pub fn open_project_folder(app: tauri::AppHandle) -> Result<String, String> {
    let folder = app.dialog().file().blocking_pick_folder();

    match folder {
        Some(folder_path) => Ok(folder_path.to_string()),
        None => Err("Usuario cancelo la accion".to_string()),
    }
}

#[derive(serde::Serialize, Debug, Clone)]
pub struct FileEntry {
    name: String,
    path: String,
    is_dir: bool,
    children: Vec<FileEntry>,
}

fn get_file_tree(base: &Path, current: &Path) -> Vec<FileEntry> {
    let mut entries = Vec::new();

    if let Ok(dir_entries) = fs::read_dir(current) {
        let mut sorted_entries: Vec<_> = dir_entries.flatten().collect();

        // Sort entries: directories first, then files, then alphabetically
        sorted_entries.sort_by(|a, b| {
            let a_is_dir = a.path().is_dir();
            let b_is_dir = b.path().is_dir();
            if a_is_dir != b_is_dir {
                b_is_dir.cmp(&a_is_dir)
            } else {
                a.file_name().cmp(&b.file_name())
            }
        });

        for entry in sorted_entries {
            let path = entry.path();
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();

            if path.is_dir() {
                let children = get_file_tree(base, &path);
                // Only add directory if it's not empty or contains markdown files (transitive)
                // Actually, for a real editor, we might want to show all folders.
                // But the user specifically mentioned "list_markdown_files" before.
                // Let's keep it simple: show all folders that contain at least one .md file deep down.
                if !children.is_empty() {
                    entries.push(FileEntry {
                        name,
                        path: path.to_string_lossy().into_owned(),
                        is_dir: true,
                        children,
                    });
                }
            } else if let Some(ext) = path.extension() {
                if ext == "md" {
                    entries.push(FileEntry {
                        name,
                        path: path.to_string_lossy().into_owned(),
                        is_dir: false,
                        children: Vec::new(),
                    });
                }
            }
        }
    }
    entries
}

#[tauri::command]
pub fn list_markdown_files(folder_path: String) -> Result<Vec<FileEntry>, String> {
    let base_path = Path::new(&folder_path);
    if !base_path.is_dir() {
        return Err("La ruta proporcionada no es una carpeta válida".to_string());
    }

    let tree = get_file_tree(base_path, base_path);

    if tree.is_empty() {
        return Err("No se encontraron archivos Markdown en la carpeta seleccionada".to_string());
    }
    Ok(tree)
}

#[tauri::command]
pub fn read_file(path_str: String) -> Result<String, String> {
    let path = Path::new(&path_str);
    fs::read_to_string(path).map_err(|e| format!("Error al leer el archivo: {}", e))
}

#[tauri::command]
pub fn save_file(path_str: String, content: String) -> Result<(), String> {
    let path = Path::new(&path_str);
    fs::write(path, content).map_err(|e| format!("Error al guardar el archivo: {}", e))
}

#[tauri::command]
pub fn generate_toc(content: String) -> String {
    crate::utils::md::generate_toc(&content)
}

#[tauri::command]
pub fn delete_file(path_str: String) -> Result<(), String> {
    let path = Path::new(&path_str);
    fs::remove_file(path).map_err(|e| format!("Error al eliminar el archivo: {}", e))
}

#[tauri::command(rename_all = "camelCase")]
pub fn rename_file(old_path: String, new_name: String) -> Result<(), String> {
    let old_path_buf = Path::new(&old_path);
    let parent = old_path_buf
        .parent()
        .ok_or("No se pudo determinar la carpeta padre")?;
    let new_path = parent.join(new_name);

    fs::rename(old_path_buf, new_path).map_err(|e| format!("Error al renombrar el archivo: {}", e))
}

#[tauri::command]
pub fn create_file(folder_path: String, name: String) -> Result<String, String> {
    let path = Path::new(&folder_path).join(name);
    if path.exists() {
        return Err("El archivo ya existe".to_string());
    }
    fs::write(&path, "# Nuevo Archivo\n")
        .map_err(|e| format!("Error al crear el archivo: {}", e))?;
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn watch_folder(app: tauri::AppHandle, folder_path: String) -> Result<(), String> {
    let mut watcher_lock = WATCHER.lock().map_err(|e| format!("Error al bloquear watcher: {}", e))?;

    if watcher_lock.is_some() {
        let _ = std::mem::take(&mut *watcher_lock);
    }

    let app_handle = app.clone();
    let mut watcher = RecommendedWatcher::new(
        move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                if matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)) {
                    let paths: Vec<String> = event.paths
                        .iter()
                        .filter(|p| p.extension().is_some_and(|ext| ext == "md"))
                        .map(|p| p.to_string_lossy().into_owned())
                        .collect();

                    if !paths.is_empty() {
                        let _ = app_handle.emit("fs-change", &paths);
                    }
                }
            }
        },
        notify::Config::default(),
    ).map_err(|e| format!("Error al crear watcher: {}", e))?;

    watcher.watch(Path::new(&folder_path), RecursiveMode::Recursive)
        .map_err(|e| format!("Error al observar carpeta: {}", e))?;

    *watcher_lock = Some(watcher);
    Ok(())
}

#[tauri::command]
pub fn stop_watching() -> Result<(), String> {
    let mut watcher_lock = WATCHER.lock().map_err(|e| format!("Error al bloquear watcher: {}", e))?;
    *watcher_lock = None;
    Ok(())
}
