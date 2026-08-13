use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sha2::Digest;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult, CommandResult};
use crate::invoice_db::PersistInvoicesOutcome;
use crate::invoice_files::{
    InspectedInvoiceFile, MAX_INVOICE_FILES, copy_invoice_source, inspect_invoice_file,
    resolve_invoice_path,
};
use crate::invoice_queue::{
    emit_update, prepare_previews, remove_duplicate_source, remove_previews,
    remove_unpersisted_previews,
};
use crate::models::*;
use crate::tencent_ocr::parse_response;

#[derive(Clone)]
struct PreparedEntry {
    path: PathBuf,
    original_file_name: String,
    fingerprint: String,
    inspected: Option<InspectedInvoiceFile>,
    status: String,
    message: Option<String>,
}
#[derive(Clone)]
struct PreparedBatch {
    created: Instant,
    forced_type: Option<i32>,
    entries: Vec<PreparedEntry>,
}
#[derive(Clone, Default)]
pub struct InvoicePreflightStore {
    values: Arc<Mutex<HashMap<String, PreparedBatch>>>,
}

#[tauri::command]
pub async fn prepare_invoice_batch(
    state: State<'_, AppState>,
    request: PrepareInvoiceBatchRequest,
) -> CommandResult<InvoiceBatchPreview> {
    prepare_inner(&state, request).await.map_err(Into::into)
}
async fn prepare_inner(
    state: &AppState,
    request: PrepareInvoiceBatchRequest,
) -> AppResult<InvoiceBatchPreview> {
    if request.file_paths.is_empty() {
        return Err(AppError::InvalidInput("请至少选择一个发票文件".into()));
    }
    if request.file_paths.len() > MAX_INVOICE_FILES {
        return Err(AppError::InvalidInput("单个批次最多导入 500 个文件".into()));
    }
    validate_forced_type(request.forced_type)?;
    let mut seen = HashSet::new();
    let mut entries = Vec::with_capacity(request.file_paths.len());
    let mut files = Vec::with_capacity(request.file_paths.len());
    let mut valid = 0;
    let mut duplicate = 0;
    let mut invalid = 0;
    let mut pages = 0;
    for raw in request.file_paths {
        let path = PathBuf::from(&raw);
        let fallback = path
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or("未知文件")
            .to_owned();
        match inspect_invoice_file(&path).await {
            Ok(value) => {
                let fingerprint = format!("sha256:{}", value.hash);
                let is_duplicate = !seen.insert(value.hash.clone())
                    || state.database.invoice_hash_exists(&value.hash).await?;
                let status = if is_duplicate { "duplicate" } else { "valid" };
                if is_duplicate {
                    duplicate += 1
                } else {
                    valid += 1;
                    pages += value.pdf_pages;
                }
                files.push(InvoicePreflightFile {
                    path: raw.clone(),
                    original_file_name: value.original_file_name.clone(),
                    media_type: value.media_type.clone(),
                    size_bytes: value.size_bytes,
                    pdf_pages: value.pdf_pages,
                    status: status.into(),
                    message: is_duplicate.then(|| "相同文件已导入或在本批次重复".into()),
                });
                entries.push(PreparedEntry {
                    path,
                    original_file_name: value.original_file_name.clone(),
                    fingerprint,
                    inspected: Some(value),
                    status: status.into(),
                    message: is_duplicate.then(|| "相同文件已导入或在本批次重复".into()),
                });
            }
            Err(error) => {
                invalid += 1;
                let message = error.to_string();
                let fingerprint = file_fingerprint(&path).await;
                files.push(InvoicePreflightFile {
                    path: raw.clone(),
                    original_file_name: fallback.clone(),
                    media_type: "unknown".into(),
                    size_bytes: 0,
                    pdf_pages: 0,
                    status: "invalid".into(),
                    message: Some(message.clone()),
                });
                entries.push(PreparedEntry {
                    path,
                    original_file_name: fallback,
                    fingerprint,
                    inspected: None,
                    status: "invalid".into(),
                    message: Some(message),
                });
            }
        }
    }
    let token = Uuid::now_v7().to_string();
    let preview = InvoiceBatchPreview {
        preflight_token: token.clone(),
        selected_count: entries.len(),
        valid_count: valid,
        duplicate_count: duplicate,
        invalid_count: invalid,
        predicted_requests: valid,
        total_pdf_pages: pages,
        forced_type: request.forced_type,
        files,
    };
    let mut cache = state.invoice_preflights.values.lock().await;
    cache.retain(|_, value| value.created.elapsed() < Duration::from_secs(600));
    cache.insert(
        token,
        PreparedBatch {
            created: Instant::now(),
            forced_type: request.forced_type,
            entries,
        },
    );
    Ok(preview)
}

#[tauri::command]
pub async fn create_invoice_batch(
    app: AppHandle,
    state: State<'_, AppState>,
    request: CreateInvoiceBatchRequest,
) -> CommandResult<InvoiceBatchDetail> {
    create_inner(&app, &state, request)
        .await
        .map_err(Into::into)
}
async fn create_inner(
    app: &AppHandle,
    state: &AppState,
    request: CreateInvoiceBatchRequest,
) -> AppResult<InvoiceBatchDetail> {
    let prepared = state
        .invoice_preflights
        .values
        .lock()
        .await
        .remove(&request.preflight_token)
        .ok_or_else(|| AppError::InvalidInput("预检已过期，请重新选择文件".into()))?;
    if prepared.created.elapsed() >= Duration::from_secs(600) {
        return Err(AppError::InvalidInput("预检已过期，请重新选择文件".into()));
    }
    let has_valid = prepared.entries.iter().any(|entry| entry.status == "valid");
    let settings = state.settings.private().await;
    if has_valid && !settings.public().tencent_credentials_configured {
        return Err(AppError::InvalidInput("请先在设置中配置腾讯云密钥".into()));
    }
    for entry in &prepared.entries {
        let fingerprint = file_fingerprint(&entry.path).await;
        if fingerprint != entry.fingerprint {
            return Err(AppError::InvalidInput(format!(
                "{} 在预检后发生变化，请重新预检",
                entry.original_file_name
            )));
        }
        if let Some(expected) = &entry.inspected {
            let current = inspect_invoice_file(&entry.path).await.map_err(|_| {
                AppError::InvalidInput(format!(
                    "{} 在预检后发生变化，请重新预检",
                    entry.original_file_name
                ))
            })?;
            if current.hash != expected.hash {
                return Err(AppError::InvalidInput(format!(
                    "{} 在预检后发生变化，请重新预检",
                    entry.original_file_name
                )));
            }
        }
    }
    let batch_id = Uuid::now_v7().to_string();
    state.invoice_queue.reserve(&batch_id).await?;
    if let Err(error) = state
        .database
        .create_invoice_batch(&batch_id, prepared.forced_type)
        .await
    {
        state.invoice_queue.release(&batch_id).await;
        return Err(error);
    }
    let mut copied = Vec::new();
    let mut scheduled = Vec::new();
    let creation: AppResult<()> = async {
        for entry in prepared.entries {
            let id = Uuid::now_v7();
            match entry.inspected {
                Some(info) if entry.status == "valid" => {
                    let relative =
                        copy_invoice_source(&entry.path, &state.app_data_dir, id, &info.extension)
                            .await?;
                    let absolute = state.app_data_dir.join(&relative);
                    let relative = relative.to_string_lossy().replace('\\', "/");
                    let accepted = state
                        .database
                        .create_invoice_source_task(
                            &id.to_string(),
                            &batch_id,
                            &entry.original_file_name,
                            &relative,
                            &info.hash,
                            &info.media_type,
                            prepared.forced_type,
                        )
                        .await;
                    let accepted = match accepted {
                        Ok(accepted) => accepted,
                        Err(error) => {
                            let _ = tokio::fs::remove_file(&absolute).await;
                            return Err(error);
                        }
                    };
                    if !accepted {
                        let _ = tokio::fs::remove_file(&absolute).await;
                        state
                            .database
                            .create_invoice_task(
                                &id.to_string(),
                                &batch_id,
                                &entry.original_file_name,
                                None,
                                &info.hash,
                                &info.media_type,
                                prepared.forced_type,
                                "failed",
                                Some("duplicate_file"),
                                Some("相同文件已在预检后被其他批次导入"),
                            )
                            .await?;
                        continue;
                    }
                    copied.push(absolute);
                    scheduled.push(id.to_string());
                }
                Some(info) => {
                    state
                        .database
                        .create_invoice_task(
                            &id.to_string(),
                            &batch_id,
                            &entry.original_file_name,
                            None,
                            &info.hash,
                            &info.media_type,
                            prepared.forced_type,
                            "failed",
                            Some("duplicate_file"),
                            entry.message.as_deref(),
                        )
                        .await?;
                }
                None => {
                    let hash = format!(
                        "invalid:{:x}",
                        sha2::Sha256::digest(entry.path.to_string_lossy().as_bytes())
                    );
                    state
                        .database
                        .create_invoice_task(
                            &id.to_string(),
                            &batch_id,
                            &entry.original_file_name,
                            None,
                            &hash,
                            "unknown",
                            prepared.forced_type,
                            "failed",
                            Some("invalid_file"),
                            entry.message.as_deref(),
                        )
                        .await?;
                }
            }
        }
        Ok(())
    }
    .await;
    if let Err(error) = creation {
        let _ = state.database.delete_invoice_batch(&batch_id).await;
        for path in copied {
            let _ = tokio::fs::remove_file(path).await;
        }
        state.invoice_queue.release(&batch_id).await;
        return Err(error);
    }
    if let Err(error) = state.database.finalize_invoice_batch(&batch_id).await {
        let _ = state.database.delete_invoice_batch(&batch_id).await;
        for path in copied {
            let _ = tokio::fs::remove_file(path).await;
        }
        state.invoice_queue.release(&batch_id).await;
        return Err(error);
    }
    let batch = state.database.invoice_batch(&batch_id).await?;
    if let Some(task) = batch.tasks.last().cloned() {
        let _ = app.emit(
            "invoice-task-updated",
            InvoiceTaskUpdateEvent {
                task,
                batch: batch.clone(),
            },
        );
    }
    state
        .invoice_queue
        .enqueue_reserved(app.clone(), batch_id, scheduled, settings)
        .await?;
    Ok(batch)
}

#[tauri::command]
pub async fn get_invoice_batch(
    state: State<'_, AppState>,
    request: IdRequest,
) -> CommandResult<InvoiceBatchDetail> {
    state
        .database
        .invoice_batch(&request.id)
        .await
        .map_err(Into::into)
}
#[tauri::command]
pub async fn list_invoice_batches(
    state: State<'_, AppState>,
    query: InvoiceBatchesQuery,
) -> CommandResult<InvoiceBatchesPage> {
    state
        .database
        .list_invoice_batches(query)
        .await
        .map_err(Into::into)
}
#[tauri::command]
pub async fn cancel_invoice_task(
    app: AppHandle,
    state: State<'_, AppState>,
    request: IdRequest,
) -> CommandResult<bool> {
    async {
        let canceled = state.database.cancel_invoice_task(&request.id).await?;
        if canceled {
            emit_update(&app, &state.database, &request.id).await?;
        }
        Ok::<_, AppError>(canceled)
    }
    .await
    .map_err(Into::into)
}
#[tauri::command]
pub async fn retry_invoice_task(
    app: AppHandle,
    state: State<'_, AppState>,
    request: RetryInvoiceTaskRequest,
) -> CommandResult<InvoiceBatchDetail> {
    async {
        validate_forced_type(request.forced_type)?;
        let current = state.database.invoice_task(&request.id).await?;
        state.invoice_queue.reserve(&current.batch_id).await?;
        let task = match state
            .database
            .prepare_invoice_retry(&request.id, request.forced_type)
            .await
        {
            Ok(value) => value,
            Err(error) => {
                state.invoice_queue.release(&current.batch_id).await;
                return Err(error);
            }
        };
        let settings = state.settings.private().await;
        state
            .invoice_queue
            .enqueue_reserved(app, task.batch_id.clone(), vec![task.id], settings)
            .await?;
        state.database.invoice_batch(&task.batch_id).await
    }
    .await
    .map_err(Into::into)
}

#[tauri::command]
pub async fn confirm_invoice_split(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ConfirmInvoiceSplitRequest,
) -> CommandResult<InvoiceBatchDetail> {
    async {
        let (task, request_id, raw) = state
            .database
            .invoice_task_raw_response(&request.task_id)
            .await?;
        if task.status != "awaiting_review" {
            return Err(AppError::InvalidInput("该任务不处于待复核状态".into()));
        }
        let mut parsed = parse_response(raw.clone())?;
        apply_groups(&mut parsed.invoices, &request.groups)?;
        let source = resolve_invoice_path(
            &state.app_data_dir,
            task.source_path
                .as_deref()
                .ok_or_else(|| AppError::File("源文件不存在".into()))?,
        )?;
        prepare_previews(&state.app_data_dir, &source, &task.media_type, &mut parsed).await?;
        let persisted = state
            .database
            .persist_invoices(
                &task.id,
                &request_id,
                &raw,
                &parsed.invoices,
                parsed.warning.as_deref(),
            )
            .await;
        let outcome = match persisted {
            Ok(value) => value,
            Err(error) => {
                remove_previews(&state.app_data_dir, &parsed).await;
                return Err(error);
            }
        };
        match outcome {
            PersistInvoicesOutcome::Inserted { invoice_ids } => {
                remove_unpersisted_previews(&state.app_data_dir, &parsed, &invoice_ids).await;
            }
            PersistInvoicesOutcome::Duplicate {
                existing_invoice_id,
            } => {
                remove_previews(&state.app_data_dir, &parsed).await;
                state
                    .database
                    .mark_invoice_failed(
                        &task.id,
                        "duplicate_invoice",
                        "相同发票代码和号码已存在",
                        Some(&request_id),
                        Some(&raw),
                        Some(&existing_invoice_id),
                    )
                    .await?;
                remove_duplicate_source(&state.database, &task.id, &source).await;
            }
            PersistInvoicesOutcome::RecoveryNoMatch {
                existing_invoice_id,
            } => {
                remove_previews(&state.app_data_dir, &parsed).await;
                state
                    .database
                    .mark_invoice_failed(
                        &task.id,
                        "recovery_no_match",
                        "本次识别未找到待补齐的已删除发票，可重新导入后重试",
                        Some(&request_id),
                        Some(&raw),
                        existing_invoice_id.as_deref(),
                    )
                    .await?;
                remove_duplicate_source(&state.database, &task.id, &source).await;
            }
        }
        emit_update(&app, &state.database, &task.id).await?;
        state.database.invoice_batch(&task.batch_id).await
    }
    .await
    .map_err(Into::into)
}

#[tauri::command]
pub async fn list_invoices(
    state: State<'_, AppState>,
    query: InvoicesQuery,
) -> CommandResult<InvoicesPage> {
    let settings = state.settings.private().await;
    state
        .database
        .list_invoices_with_matching(
            query,
            usize::from(settings.match_max_orders),
            usize::from(settings.match_max_candidates),
        )
        .await
        .map_err(Into::into)
}
#[tauri::command]
pub async fn get_invoice(
    state: State<'_, AppState>,
    request: IdRequest,
) -> CommandResult<InvoiceDetail> {
    let mut value = state
        .database
        .invoice_detail(&request.id)
        .await
        .map_err(crate::error::CommandError::from)?;
    resolve_detail_paths(&state, &mut value).map_err(crate::error::CommandError::from)?;
    Ok(value)
}
#[tauri::command]
pub async fn update_invoice(
    state: State<'_, AppState>,
    request: UpdateInvoiceRequest,
) -> CommandResult<InvoiceDetail> {
    let mut value = state
        .database
        .update_invoice(request)
        .await
        .map_err(crate::error::CommandError::from)?;
    resolve_detail_paths(&state, &mut value).map_err(crate::error::CommandError::from)?;
    Ok(value)
}
#[tauri::command]
pub async fn delete_invoice(state: State<'_, AppState>, request: IdRequest) -> CommandResult<()> {
    delete_one(&state, &request.id).await.map_err(Into::into)
}
#[tauri::command]
pub async fn delete_invoices(
    state: State<'_, AppState>,
    request: DeleteInvoicesRequest,
) -> CommandResult<DeleteInvoicesResult> {
    async {
        if request.ids.is_empty() || request.ids.len() > 100 {
            return Err(AppError::InvalidInput("单次请选择 1 到 100 张发票".into()));
        }
        let mut seen = HashSet::new();
        let mut results = Vec::new();
        for id in request.ids {
            if !seen.insert(id.clone()) {
                continue;
            }
            match delete_one(&state, &id).await {
                Ok(()) => results.push(DeleteInvoiceResult {
                    id,
                    deleted: true,
                    error_code: None,
                    error_message: None,
                }),
                Err(error) => results.push(DeleteInvoiceResult {
                    id,
                    deleted: false,
                    error_code: Some(error.code().into()),
                    error_message: Some(error.to_string()),
                }),
            }
        }
        Ok::<_, AppError>(DeleteInvoicesResult { results })
    }
    .await
    .map_err(Into::into)
}

async fn delete_one(state: &AppState, id: &str) -> AppResult<()> {
    let (_task, preview, source, last_reference) =
        state.database.invoice_paths_for_delete(id).await?;
    let preview = resolve_invoice_path(&state.app_data_dir, &preview)?;
    let source = (!source.is_empty())
        .then(|| resolve_invoice_path(&state.app_data_dir, &source))
        .transpose()?;
    let trash = state.app_data_dir.join("trash");
    tokio::fs::create_dir_all(&trash).await?;
    let preview_trash = trash.join(format!("invoice-{id}.preview.deleting"));
    tokio::fs::rename(&preview, &preview_trash).await?;
    let mut source_trash = None;
    if last_reference && let Some(path) = source.as_ref().filter(|path| path.exists()) {
        let target = trash.join(format!("invoice-{id}.source.deleting"));
        if let Err(error) = tokio::fs::rename(path, &target).await {
            let _ = tokio::fs::rename(&preview_trash, &preview).await;
            return Err(error.into());
        }
        source_trash = Some(target);
    }
    if let Err(error) = state.database.delete_invoice_record(id).await {
        let preview_restore = tokio::fs::rename(&preview_trash, &preview).await;
        if let (Some(trash_path), Some(source_path)) = (&source_trash, &source) {
            let _ = tokio::fs::rename(trash_path, source_path).await;
        }
        if let Err(restore_error) = preview_restore {
            return Err(AppError::Internal(format!(
                "删除数据库记录失败，且无法恢复发票预览：{error}；{restore_error}"
            )));
        }
        return Err(error);
    }
    if let Err(error) = tokio::fs::remove_file(&preview_trash).await {
        tracing::warn!(invoice_id=id,error=%error,"failed to remove invoice preview trash");
    }
    if let Some(path) = source_trash
        && let Err(error) = tokio::fs::remove_file(path).await
    {
        tracing::warn!(invoice_id=id,error=%error,"failed to remove invoice source trash");
    }
    Ok(())
}
fn resolve_detail_paths(state: &AppState, value: &mut InvoiceDetail) -> AppResult<()> {
    value.preview_path = resolve_invoice_path(&state.app_data_dir, &value.preview_path)?
        .to_string_lossy()
        .into_owned();
    value.source_path = resolve_invoice_path(&state.app_data_dir, &value.source_path)?
        .to_string_lossy()
        .into_owned();
    Ok(())
}
fn validate_forced_type(value: Option<i32>) -> AppResult<()> {
    const TYPES: &[i32] = &[
        -1, 0, 1, 2, 3, 5, 8, 9, 10, 11, 12, 13, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    ];
    if value.is_some_and(|v| !TYPES.contains(&v)) {
        return Err(AppError::InvalidInput("不支持的腾讯云发票类型".into()));
    }
    Ok(())
}

async fn file_fingerprint(path: &std::path::Path) -> String {
    if let Ok(bytes) = tokio::fs::read(path).await {
        return format!("sha256:{:x}", sha2::Sha256::digest(bytes));
    }
    match tokio::fs::metadata(path).await {
        Ok(metadata) => {
            let modified = metadata
                .modified()
                .ok()
                .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|value| value.as_nanos())
                .unwrap_or_default();
            format!("unreadable:{}:{modified}", metadata.len())
        }
        Err(_) => "missing".into(),
    }
}
fn apply_groups(invoices: &mut Vec<NormalizedInvoice>, groups: &[Vec<usize>]) -> AppResult<()> {
    if groups.is_empty() {
        return Err(AppError::InvalidInput("请至少保留一个发票分组".into()));
    }
    let original = invoices.clone();
    let mut used = HashSet::new();
    let mut result = Vec::new();
    for group in groups {
        if group.is_empty() {
            return Err(AppError::InvalidInput("发票分组不能为空".into()));
        }
        let mut value = original
            .get(group[0])
            .cloned()
            .ok_or_else(|| AppError::InvalidInput("分组索引超出识别结果".into()))?;
        let group_identity = value.reliable_identity.clone();
        let mut identity_consistent = true;
        for (position, index) in group.iter().enumerate() {
            if !used.insert(*index) {
                return Err(AppError::InvalidInput("同一识别项不能属于多个分组".into()));
            }
            let item = original
                .get(*index)
                .ok_or_else(|| AppError::InvalidInput("分组索引超出识别结果".into()))?;
            identity_consistent &= item.reliable_identity == group_identity;
            value.page_start = value.page_start.min(item.page_start);
            value.page_end = value.page_end.max(item.page_end);
            if position > 0 {
                value.items.extend(item.items.clone());
                let first = serde_json::from_str::<serde_json::Value>(&value.raw_invoice_json)
                    .unwrap_or_default();
                let second = serde_json::from_str::<serde_json::Value>(&item.raw_invoice_json)
                    .unwrap_or_default();
                let mut merged = match first {
                    serde_json::Value::Array(values) => values,
                    value => vec![value],
                };
                match second {
                    serde_json::Value::Array(values) => merged.extend(values),
                    value => merged.push(value),
                }
                value.raw_invoice_json = serde_json::Value::Array(merged).to_string();
            }
        }
        if !identity_consistent {
            value.reliable_identity = None;
            value.possible_duplicate = true;
        }
        result.push(value);
    }
    if used.len() != original.len() {
        return Err(AppError::InvalidInput("每个识别项都必须分组".into()));
    }
    let mut identities = HashSet::new();
    if result
        .iter()
        .filter_map(|value| value.reliable_identity.as_ref())
        .any(|identity| !identities.insert(identity.clone()))
    {
        return Err(AppError::InvalidInput(
            "相同可靠发票身份不能拆成多个分组，请合并后再确认".into(),
        ));
    }
    *invoices = result;
    Ok(())
}

#[cfg(test)]
mod tests {
    use sea_orm::{EntityTrait, PaginatorTrait};
    use tempfile::TempDir;

    use super::*;
    use crate::db::Database;

    async fn test_state() -> (TempDir, AppState) {
        let directory = tempfile::tempdir().unwrap();
        let app_data_dir = directory.path().to_path_buf();
        let images_dir = app_data_dir.join("images");
        tokio::fs::create_dir_all(&images_dir).await.unwrap();
        let database = Database::in_memory().await.unwrap();
        let state = AppState::test(database, app_data_dir, images_dir).await;
        (directory, state)
    }

    #[tokio::test]
    async fn delete_restores_invoice_files_when_database_delete_fails() {
        let (_directory, state) = test_state().await;
        let source_relative = "invoices/sources/task.png";
        let preview_relative = "invoices/previews/invoice.png";
        let source = state.app_data_dir.join(source_relative);
        let preview = state.app_data_dir.join(preview_relative);
        tokio::fs::create_dir_all(source.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::create_dir_all(preview.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&source, b"source").await.unwrap();
        tokio::fs::write(&preview, b"preview").await.unwrap();
        state
            .database
            .create_invoice_batch("batch", None)
            .await
            .unwrap();
        state
            .database
            .create_invoice_task(
                "task",
                "batch",
                "invoice.png",
                Some(source_relative),
                "hash",
                "image/png",
                None,
                "queued",
                None,
                None,
            )
            .await
            .unwrap();
        assert!(
            state
                .database
                .mark_invoice_processing("task")
                .await
                .unwrap()
        );
        let invoice = NormalizedInvoice {
            id: "invoice".into(),
            type_id: 3,
            subtype: "VatSpecialInvoice".into(),
            type_description: "增值税发票".into(),
            subtype_description: "增值税专用发票".into(),
            invoice_code: Some("4403".into()),
            invoice_number: Some("1001".into()),
            invoice_date: Some("2026-08-12".into()),
            buyer_name: None,
            buyer_tax_id: None,
            seller_name: None,
            seller_tax_id: None,
            amount_fen: Some(100),
            tax_fen: Some(6),
            total_fen: Some(106),
            currency: "CNY".into(),
            reliable_identity: crate::invoice_db::reliable_identity(
                "VatSpecialInvoice",
                Some("4403"),
                Some("1001"),
            ),
            possible_duplicate: false,
            preview_path: preview_relative.into(),
            page_start: 1,
            page_end: 1,
            raw_invoice_json: "{}".into(),
            items: Vec::new(),
        };
        state
            .database
            .persist_invoices("task", "request", "{}", &[invoice], None)
            .await
            .unwrap();
        state
            .database
            .execute_test_statement(
                "CREATE TRIGGER fail_invoice_task_delete BEFORE DELETE ON invoice_tasks \
                 WHEN OLD.id = 'task' BEGIN SELECT RAISE(FAIL, 'forced delete failure'); END",
            )
            .await
            .unwrap();

        assert!(delete_one(&state, "invoice").await.is_err());
        assert!(source.exists());
        assert!(preview.exists());
        assert!(state.database.invoice_detail("invoice").await.is_ok());
        assert_eq!(
            crate::entities::invoice_recovery_marker::Entity::find()
                .count(&state.database.connection)
                .await
                .unwrap(),
            0
        );
    }

    #[test]
    fn forced_type_allowlist_matches_tencent_request_contract() {
        for value in [
            -1, 0, 1, 2, 3, 5, 8, 9, 10, 11, 12, 13, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
        ] {
            validate_forced_type(Some(value)).unwrap();
        }
        assert!(validate_forced_type(Some(26)).is_err());
        assert!(validate_forced_type(Some(4)).is_err());
    }
}
