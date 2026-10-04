mod commands;
mod state;

use crate::commands::{
    create_file, delete_file, list_markdown_files, open_project_folder, read_file, rename_file,
    save_file, stop_watching, watch_folder,
};
use crate::state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            open_project_folder,
            list_markdown_files,
            read_file,
            save_file,
            delete_file,
            rename_file,
            create_file,
            watch_folder,
            stop_watching,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
