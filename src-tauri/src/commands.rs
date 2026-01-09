use tauri_plugin_dialog::DialogExt;

#[tauri::command]
pub fn open_project_folder(app: tauri::AppHandle) -> Result<String, String> {
    let folder = app.dialog().file().blocking_pick_folder();

    match folder {
        Some(folder_path) => Ok(folder_path.to_string()),
        None => Err("Usuario cancelo la accion".to_string()),
    }

}