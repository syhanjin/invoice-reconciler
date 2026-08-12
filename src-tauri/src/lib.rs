mod commands;
mod db;
mod entities;
mod error;
mod files;
mod invoice_commands;
mod invoice_db;
mod invoice_files;
mod invoice_queue;
mod migration;
mod models;
mod normalize;
mod ocr;
mod queue;
mod settings;
mod tencent_ocr;

use std::path::PathBuf;

use db::Database;
use error::AppError;
use ocr::OllamaClient;
use queue::QueueManager;
use settings::SettingsStore;
use tauri::Manager;

pub struct AppState {
    database: Database,
    ollama: OllamaClient,
    queue: QueueManager,
    invoice_queue: invoice_queue::InvoiceQueueManager,
    invoice_preflights: invoice_commands::InvoicePreflightStore,
    settings: SettingsStore,
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
            let legacy_settings =
                tauri::async_runtime::block_on(database.settings()).map_err(to_boxed_error)?;
            let settings = tauri::async_runtime::block_on(SettingsStore::load_or_migrate(
                &app_data_dir,
                legacy_settings,
            ))
            .map_err(to_boxed_error)?;
            let queue = QueueManager::new(database.clone(), ollama.clone(), app_data_dir.clone());
            let tencent = tencent_ocr::TencentOcrClient::new().map_err(to_boxed_error)?;
            let invoice_queue = invoice_queue::InvoiceQueueManager::new(
                database.clone(),
                tencent,
                app_data_dir.clone(),
            );
            app.manage(AppState {
                database,
                ollama,
                queue,
                invoice_queue,
                invoice_preflights: invoice_commands::InvoicePreflightStore::default(),
                settings,
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
            commands::list_import_batches,
            commands::cancel_task,
            commands::cancel_batch,
            commands::retry_task,
            commands::retry_failed_batch,
            commands::list_orders,
            commands::get_order,
            commands::update_order,
            commands::delete_order,
            commands::delete_orders,
            invoice_commands::prepare_invoice_batch,
            invoice_commands::create_invoice_batch,
            invoice_commands::get_invoice_batch,
            invoice_commands::list_invoice_batches,
            invoice_commands::cancel_invoice_task,
            invoice_commands::retry_invoice_task,
            invoice_commands::confirm_invoice_split,
            invoice_commands::list_invoices,
            invoice_commands::get_invoice,
            invoice_commands::update_invoice,
            invoice_commands::delete_invoice,
            invoice_commands::delete_invoices,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Bill");
}

fn to_boxed_error(error: AppError) -> Box<dyn std::error::Error> {
    Box::new(error)
}
