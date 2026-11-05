// Hides the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use pdfkit_core::{FileInfo, ToolResult};
use tauri::async_runtime::spawn_blocking;
use tauri_plugin_opener::OpenerExt;

/// Runs a tool off the UI thread; large PDFs can take a few seconds.
#[tauri::command]
async fn run_tool(tool: String, inputs: Vec<String>, options: serde_json::Value) -> Result<ToolResult, String> {
    spawn_blocking(move || pdfkit_core::run_tool(&tool, &inputs, &options))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn inspect_files(paths: Vec<String>) -> Result<Vec<FileInfo>, String> {
    spawn_blocking(move || pdfkit_core::inspect_files(&paths)).await.map_err(|e| e.to_string())
}

#[tauri::command]
fn open_path(app: tauri::AppHandle, path: String) -> Result<(), String> {
    app.opener().open_path(path, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
fn reveal_path(app: tauri::AppHandle, path: String) -> Result<(), String> {
    app.opener().reveal_item_in_dir(path).map_err(|e| e.to_string())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![run_tool, inspect_files, open_path, reveal_path])
        .run(tauri::generate_context!())
        .expect("PDFKit gagal dijalankan");
}
