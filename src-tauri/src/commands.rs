use std::{fs, path::{self, Path}};

use tauri_plugin_dialog::DialogExt;
use pulldown_cmark::{Parser, html};

#[tauri::command]
pub fn open_project_folder(app: tauri::AppHandle) -> Result<String, String> {
    let folder = app.dialog().file().blocking_pick_folder();

    match folder {
        Some(folder_path) => Ok(folder_path.to_string()),
        None => Err("Usuario cancelo la accion".to_string()),
    }
}

fn collect_md_files(base: &Path, current: &Path, files: &mut Vec<String>) {
    // Lógica para recopilar archivos .md
    if let Ok(entries) = fs::read_dir(current) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_md_files(base, &path, files);
            } else if let Some(ext) = path.extension() {
                if ext == "md" {
                    if let Ok(rel_path) = path.strip_prefix(base) {
                        files.push(rel_path.to_string_lossy().into_owned());
                    }
                }
            }
        }
    }
}

#[tauri::command]
pub fn list_markdown_files(folder_path: String) -> Result<Vec<String>, String> {
    let base_path = Path::new(&folder_path);
    if !base_path.is_dir() {
        return Err("La ruta proporcionada no es una carpeta válida".to_string());
    }

    let mut md_files = Vec::new();
    collect_md_files(base_path, base_path, &mut md_files);

    if (md_files.is_empty()) {
        return Err("No se encontraron archivos Markdown en la carpeta seleccionada".to_string());
    }
    Ok(md_files)
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
pub fn render_markdown(content: String) -> String {
    let parser = Parser::new(&content);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    html_output
}