mod commands;
mod db;
mod entities;
mod error;
mod files;
mod migration;
mod models;
mod normalize;
mod ocr;
mod queue;

use std::path::PathBuf;

use db::Database;
use error::AppError;
use ocr::OllamaClient;
use queue::QueueManager;
use tauri::Manager;

pub struct AppState {
    database: Database,
    ollama: OllamaClient,
    queue: QueueManager,
    app_data_dir: PathBuf,
    images_dir: PathBuf,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "bill=info".into()),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            let database_path = app_data_dir.join("bill.sqlite3");
            let images_dir = app_data_dir.join("images");
            let ollama = OllamaClient::new().map_err(to_boxed_error)?;
            let database = tauri::async_runtime::block_on(Database::connect(&database_path))
                .map_err(to_boxed_error)?;
            let queue = QueueManager::new(database.clone(), ollama.clone(), app_data_dir.clone());
            app.manage(AppState {
                database,
                ollama,
                queue,
                app_data_dir,
                images_dir,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::get_settings,
            commands::save_settings,
            commands::check_ollama,
            commands::create_import_batch,
            commands::get_batch,
            commands::cancel_task,
            commands::cancel_batch,
            commands::retry_task,
            commands::retry_failed_batch,
            commands::list_orders,
            commands::get_order,
            commands::update_order,
            commands::delete_order,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Bill");
}

fn to_boxed_error(error: AppError) -> Box<dyn std::error::Error> {
    Box::new(error)
}
