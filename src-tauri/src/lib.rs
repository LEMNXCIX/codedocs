// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

mod commands;
mod utils;
use crate::commands::{
    create_file, delete_file, generate_toc, list_markdown_files, open_project_folder, read_file,
    rename_file, render_markdown, save_file,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            open_project_folder,
            list_markdown_files,
            save_file,
            read_file,
            render_markdown,
            generate_toc,
            delete_file,
            rename_file,
            create_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
