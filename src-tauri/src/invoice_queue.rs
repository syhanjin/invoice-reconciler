use std::path::PathBuf;
use std::sync::Arc;

use futures::{StreamExt, stream};
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;

use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::invoice_db::PersistInvoicesOutcome;
use crate::invoice_files::{
    copy_invoice_preview, crop_invoice_polygon, extract_pdf_pages, resolve_invoice_path,
    write_cut_image,
};
use crate::models::{InvoiceTaskUpdateEvent, PrivateAppSettings};
use crate::tencent_ocr::{TencentOcrClient, TencentOcrResult};

#[derive(Clone)]
pub struct InvoiceQueueManager {
    database: Database,
    client: TencentOcrClient,
    app_data_dir: PathBuf,
    active_batch: Arc<Mutex<Option<String>>>,
}

impl InvoiceQueueManager {
    pub fn new(database: Database, client: TencentOcrClient, app_data_dir: PathBuf) -> Self {
        Self {
            database,
            client,
            app_data_dir,
            active_batch: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn reserve(&self, batch_id: &str) -> AppResult<()> {
        let mut active = self.active_batch.lock().await;
        if active.is_some() {
            return Err(AppError::BatchBusy);
        }
        *active = Some(batch_id.to_owned());
        Ok(())
    }

    pub async fn release(&self, batch_id: &str) {
        let mut active = self.active_batch.lock().await;
        if active.as_deref() == Some(batch_id) {
            *active = None;
        }
    }

    pub async fn enqueue_reserved(
        &self,
        app: AppHandle,
        batch_id: String,
        task_ids: Vec<String>,
        settings: PrivateAppSettings,
    ) -> AppResult<()> {
        if task_ids.is_empty() {
            self.release(&batch_id).await;
            return Ok(());
        }
        let manager = self.clone();
        tauri::async_runtime::spawn(async move {
            let concurrency = settings.invoice_ocr_concurrency.clamp(1, 5) as usize;
            stream::iter(task_ids)
                .for_each_concurrent(Some(concurrency), |task_id| {
                    let manager = manager.clone();
                    let app = app.clone();
                    let settings = settings.clone();
                    async move {
                        manager.process(&app, &task_id, &settings).await;
                    }
                })
                .await;
            manager.release(&batch_id).await;
        });
        Ok(())
    }

    async fn process(&self, app: &AppHandle, task_id: &str, settings: &PrivateAppSettings) {
        let result = self.process_inner(task_id, settings).await;
        if let Err(error) = result {
            let _ = self
                .database
                .mark_invoice_failed(task_id, error.code(), &error.to_string(), None, None, None)
                .await;
        }
        let _ = emit_update(app, &self.database, task_id).await;
    }

    async fn process_inner(&self, task_id: &str, settings: &PrivateAppSettings) -> AppResult<()> {
        if !self.database.mark_invoice_processing(task_id).await? {
            return Ok(());
        }
        let task = self.database.invoice_task(task_id).await?;
        let relative = task
            .source_path
            .as_deref()
            .ok_or_else(|| AppError::File("任务没有受管源文件".into()))?;
        let source = resolve_invoice_path(&self.app_data_dir, relative)?;
        let bytes = tokio::fs::read(&source).await?;
        let mut result = self
            .client
            .recognize(&bytes, task.forced_type, settings)
            .await?;
        if needs_manual_review(&result, &task.media_type) {
            let warning = match result.warning.as_deref() {
                Some(partial) => {
                    format!("识别结果包含无法自动拆分的跨页或混贴发票，请复核分组；{partial}")
                }
                None => "识别结果包含无法自动拆分的跨页或混贴发票，请复核分组".into(),
            };
            return self
                .database
                .mark_invoice_awaiting_review(
                    task_id,
                    &result.request_id,
                    &result.raw_response,
                    &warning,
                )
                .await;
        }
        prepare_previews(&self.app_data_dir, &source, &task.media_type, &mut result).await?;
        let persisted = self
            .database
            .persist_invoices(
                task_id,
                &result.request_id,
                &result.raw_response,
                &result.invoices,
                result.warning.as_deref(),
            )
            .await;
        let outcome = match persisted {
            Ok(value) => value,
            Err(error) => {
                remove_previews(&self.app_data_dir, &result).await;
                return Err(error);
            }
        };
        match outcome {
            PersistInvoicesOutcome::Inserted { invoice_ids } => {
                remove_unpersisted_previews(&self.app_data_dir, &result, &invoice_ids).await;
            }
            PersistInvoicesOutcome::Duplicate {
                existing_invoice_id,
            } => {
                remove_previews(&self.app_data_dir, &result).await;
                self.database
                    .mark_invoice_failed(
                        task_id,
                        "duplicate_invoice",
                        "相同发票代码和号码已存在",
                        Some(&result.request_id),
                        Some(&result.raw_response),
                        Some(&existing_invoice_id),
                    )
                    .await?;
                remove_duplicate_source(&self.database, task_id, &source).await;
            }
            PersistInvoicesOutcome::RecoveryNoMatch {
                existing_invoice_id,
            } => {
                remove_previews(&self.app_data_dir, &result).await;
                self.database
                    .mark_invoice_failed(
                        task_id,
                        "recovery_no_match",
                        "本次识别未找到待补齐的已删除发票，可重新导入后重试",
                        Some(&result.request_id),
                        Some(&result.raw_response),
                        existing_invoice_id.as_deref(),
                    )
                    .await?;
                remove_duplicate_source(&self.database, task_id, &source).await;
            }
        }
        Ok(())
    }
}

fn needs_manual_review(result: &TencentOcrResult, media_type: &str) -> bool {
    if result.requires_review {
        return true;
    }
    if result.invoices.len() <= 1 {
        return false;
    }
    result.invoices.iter().any(|invoice| {
        let invoices_on_page = result
            .invoices
            .iter()
            .filter(|candidate| candidate.page_start == invoice.page_start)
            .count();
        if invoices_on_page <= 1 {
            return false;
        }
        let raw: serde_json::Value =
            serde_json::from_str(&invoice.raw_invoice_json).unwrap_or_default();
        let cut = raw
            .get("CutImage")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        cut.is_empty() && (media_type == "application/pdf" || raw.get("Polygon").is_none())
    })
}

pub async fn prepare_previews(
    app_data: &std::path::Path,
    source: &std::path::Path,
    media_type: &str,
    result: &mut TencentOcrResult,
) -> AppResult<()> {
    let mut created = Vec::new();
    for invoice in &mut result.invoices {
        let raw: serde_json::Value =
            serde_json::from_str(&invoice.raw_invoice_json).unwrap_or_default();
        let cut = raw
            .get("CutImage")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let preview = if !cut.is_empty() {
            write_cut_image(cut, app_data, &invoice.id).await
        } else if media_type == "application/pdf" {
            let pages = (invoice.page_start..=invoice.page_end)
                .map(|value| value as u32)
                .collect::<Vec<_>>();
            extract_pdf_pages(source, app_data, &invoice.id, &pages)
        } else if let Some(polygon) = raw.get("Polygon") {
            crop_invoice_polygon(source, polygon, app_data, &invoice.id).await
        } else {
            let extension = source.extension().and_then(|v| v.to_str()).unwrap_or("png");
            copy_invoice_preview(source, app_data, &invoice.id, extension).await
        };
        let relative = match preview {
            Ok(value) => value,
            Err(error) => {
                for path in created {
                    let _ = tokio::fs::remove_file(path).await;
                }
                return Err(error);
            }
        };
        created.push(app_data.join(&relative));
        invoice.preview_path = relative.to_string_lossy().replace('\\', "/");
    }
    Ok(())
}

pub(crate) async fn remove_previews(app_data: &std::path::Path, result: &TencentOcrResult) {
    for invoice in &result.invoices {
        if let Ok(path) = resolve_invoice_path(app_data, &invoice.preview_path) {
            let _ = tokio::fs::remove_file(path).await;
        }
    }
}

pub(crate) async fn remove_unpersisted_previews(
    app_data: &std::path::Path,
    result: &TencentOcrResult,
    persisted_ids: &std::collections::HashSet<String>,
) {
    for invoice in &result.invoices {
        if persisted_ids.contains(&invoice.id) {
            continue;
        }
        if let Ok(path) = resolve_invoice_path(app_data, &invoice.preview_path) {
            let _ = tokio::fs::remove_file(path).await;
        }
    }
}

pub(crate) async fn remove_duplicate_source(
    database: &Database,
    task_id: &str,
    source: &std::path::Path,
) {
    if let Err(error) = database.release_invoice_recovery_claims(task_id).await {
        tracing::warn!(task_id,error=%error,"failed to release invoice recovery claims");
        return;
    }
    match tokio::fs::remove_file(source).await {
        Ok(()) => {
            if let Err(error) = database.clear_invoice_source_path(task_id).await {
                tracing::warn!(task_id,error=%error,"failed to clear duplicate invoice source path");
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if let Err(error) = database.clear_invoice_source_path(task_id).await {
                tracing::warn!(task_id,error=%error,"failed to clear missing duplicate source path");
            }
        }
        Err(error) => {
            tracing::warn!(task_id,error=%error,"failed to remove duplicate invoice source");
        }
    }
}

pub async fn emit_update(app: &AppHandle, database: &Database, task_id: &str) -> AppResult<()> {
    let task = database.invoice_task(task_id).await?;
    let batch = database.invoice_batch(&task.batch_id).await?;
    app.emit(
        "invoice-task-updated",
        InvoiceTaskUpdateEvent { task, batch },
    )
    .map_err(|error| AppError::Internal(error.to_string()))
}
