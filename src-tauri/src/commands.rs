use std::collections::HashSet;
use std::path::{Path, PathBuf};

use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult, CommandResult};
use crate::files::{copy_managed_image, normalized_extension, resolve_managed_path, sha256_file};
use crate::models::{
    AppSettings, BatchDetail, BootstrapState, CreateBatchRequest, IdRequest, ImportBatchesPage,
    ImportBatchesQuery, OllamaStatus, OrderDetail, OrdersPage, OrdersQuery, TaskUpdateEvent,
    UpdateOrderRequest, UpdateSettingsRequest,
};

struct PreparedImage {
    source: PathBuf,
    original_file_name: String,
    extension: String,
    hash: String,
    duplicate: bool,
}

#[tauri::command]
pub async fn bootstrap(state: State<'_, AppState>) -> CommandResult<BootstrapState> {
    async {
        Ok::<BootstrapState, AppError>(BootstrapState {
            settings: state.database.settings().await?,
            latest_batch: state.database.latest_batch().await?,
        })
    }
    .await
    .map_err(Into::into)
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> CommandResult<AppSettings> {
    state.database.settings().await.map_err(Into::into)
}

#[tauri::command]
pub async fn save_settings(
    state: State<'_, AppState>,
    request: UpdateSettingsRequest,
) -> CommandResult<AppSettings> {
    state
        .database
        .update_settings(request)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn check_ollama(
    state: State<'_, AppState>,
    request: UpdateSettingsRequest,
) -> CommandResult<OllamaStatus> {
    async {
        let settings = request.validate()?;
        state.ollama.check(&settings).await
    }
    .await
    .map_err(Into::into)
}

#[tauri::command]
pub async fn create_import_batch(
    app: AppHandle,
    state: State<'_, AppState>,
    request: CreateBatchRequest,
) -> CommandResult<BatchDetail> {
    create_import_batch_inner(&app, &state, request)
        .await
        .map_err(Into::into)
}

async fn create_import_batch_inner(
    app: &AppHandle,
    state: &AppState,
    request: CreateBatchRequest,
) -> AppResult<BatchDetail> {
    if request.image_paths.is_empty() {
        return Err(AppError::InvalidInput("请至少选择一张订单截图".into()));
    }
    if request.image_paths.len() > 500 {
        return Err(AppError::InvalidInput("单个批次最多导入 500 张图片".into()));
    }

    let settings = state.database.settings().await?;
    let check = state.ollama.check(&settings).await?;
    if !check.model_available {
        return Err(AppError::ModelMissing(settings.ollama_model.clone()));
    }

    let mut batch_hashes = HashSet::new();
    let mut prepared = Vec::with_capacity(request.image_paths.len());
    for source in request.image_paths {
        let source = PathBuf::from(source);
        let original_file_name = source
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| AppError::InvalidInput("图片文件名不是有效 UTF-8".into()))?
            .to_owned();
        let extension = normalized_extension(&source)?;
        let hash = sha256_file(&source).await?;
        let duplicate =
            !batch_hashes.insert(hash.clone()) || state.database.hash_exists(&hash).await?;
        prepared.push(PreparedImage {
            source,
            original_file_name,
            extension,
            hash,
            duplicate,
        });
    }

    let batch_id = Uuid::now_v7().to_string();
    state.queue.reserve(&batch_id).await?;
    if let Err(error) = state
        .database
        .create_batch(&batch_id, request.platform)
        .await
    {
        state.queue.release(&batch_id).await;
        return Err(error);
    }
    let mut scheduled = Vec::new();
    let mut copied_paths = Vec::new();
    let creation_result: AppResult<()> = async {
        for image in prepared {
            let id = Uuid::now_v7();
            if image.duplicate {
                state
                    .database
                    .create_task(
                        &id.to_string(),
                        &batch_id,
                        &image.original_file_name,
                        None,
                        &image.hash,
                        "skipped",
                        Some("相同图片已导入，本次已跳过"),
                    )
                    .await?;
                continue;
            }

            let relative =
                copy_managed_image(&image.source, &state.images_dir, id, &image.extension).await?;
            let relative_string = relative.to_string_lossy().replace('\\', "/");
            copied_paths.push(state.app_data_dir.join(&relative));
            state
                .database
                .create_task(
                    &id.to_string(),
                    &batch_id,
                    &image.original_file_name,
                    Some(&relative_string),
                    &image.hash,
                    "queued",
                    None,
                )
                .await?;
            scheduled.push(id.to_string());
        }
        Ok(())
    }
    .await;

    if let Err(error) = creation_result {
        let _ = state.database.delete_batch(&batch_id).await;
        for path in copied_paths {
            let _ = tokio::fs::remove_file(path).await;
        }
        state.queue.release(&batch_id).await;
        return Err(error);
    }

    if let Err(error) = state.database.finalize_batch(&batch_id).await {
        state.queue.release(&batch_id).await;
        return Err(error);
    }
    let batch = state.database.batch(&batch_id).await?;
    if let Some(task) = batch.tasks.last().cloned() {
        let _ = app.emit(
            "import-task-updated",
            TaskUpdateEvent {
                task,
                batch: batch.clone(),
            },
        );
    }
    state
        .queue
        .enqueue_reserved(app.clone(), batch_id, scheduled, settings)
        .await?;
    Ok(batch)
}

#[tauri::command]
pub async fn get_batch(
    state: State<'_, AppState>,
    request: IdRequest,
) -> CommandResult<BatchDetail> {
    state.database.batch(&request.id).await.map_err(Into::into)
}

#[tauri::command]
pub async fn list_import_batches(
    state: State<'_, AppState>,
    query: ImportBatchesQuery,
) -> CommandResult<ImportBatchesPage> {
    state
        .database
        .list_import_batches(query)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn cancel_task(
    app: AppHandle,
    state: State<'_, AppState>,
    request: IdRequest,
) -> CommandResult<bool> {
    async {
        let canceled = state.database.mark_canceled(&request.id).await?;
        if canceled {
            emit_task_update(&app, &state, &request.id).await?;
        }
        Ok::<bool, AppError>(canceled)
    }
    .await
    .map_err(Into::into)
}

#[tauri::command]
pub async fn cancel_batch(
    app: AppHandle,
    state: State<'_, AppState>,
    request: IdRequest,
) -> CommandResult<u64> {
    async {
        let affected = state.database.cancel_batch_queued(&request.id).await?;
        if let Ok(batch) = state.database.batch(&request.id).await
            && let Some(task) = batch.tasks.last().cloned()
        {
            let _ = app.emit("import-task-updated", TaskUpdateEvent { task, batch });
        }
        Ok::<u64, AppError>(affected)
    }
    .await
    .map_err(Into::into)
}

#[tauri::command]
pub async fn retry_task(
    app: AppHandle,
    state: State<'_, AppState>,
    request: IdRequest,
) -> CommandResult<BatchDetail> {
    async {
        let current = state.database.task(&request.id).await?;
        state.queue.reserve(&current.batch_id).await?;
        let settings = match state.database.settings().await {
            Ok(settings) => settings,
            Err(error) => {
                state.queue.release(&current.batch_id).await;
                return Err(error);
            }
        };
        let task = match state.database.prepare_retry(&request.id).await {
            Ok(task) => task,
            Err(error) => {
                state.queue.release(&current.batch_id).await;
                return Err(error);
            }
        };
        state
            .queue
            .enqueue_reserved(app, task.batch_id.clone(), vec![task.id], settings)
            .await?;
        state.database.batch(&task.batch_id).await
    }
    .await
    .map_err(Into::into)
}

#[tauri::command]
pub async fn retry_failed_batch(
    app: AppHandle,
    state: State<'_, AppState>,
    request: IdRequest,
) -> CommandResult<BatchDetail> {
    async {
        let ids = state.database.retryable_task_ids(&request.id).await?;
        if ids.is_empty() {
            return Err(AppError::InvalidInput("该批次没有可重试任务".into()));
        }
        state.queue.reserve(&request.id).await?;
        let settings = match state.database.settings().await {
            Ok(settings) => settings,
            Err(error) => {
                state.queue.release(&request.id).await;
                return Err(error);
            }
        };
        if let Err(error) = state.database.prepare_retries(&ids).await {
            state.queue.release(&request.id).await;
            return Err(error);
        }
        state
            .queue
            .enqueue_reserved(app, request.id.clone(), ids, settings)
            .await?;
        state.database.batch(&request.id).await
    }
    .await
    .map_err(Into::into)
}

#[tauri::command]
pub async fn list_orders(
    state: State<'_, AppState>,
    query: OrdersQuery,
) -> CommandResult<OrdersPage> {
    state.database.list_orders(query).await.map_err(Into::into)
}

#[tauri::command]
pub async fn get_order(
    state: State<'_, AppState>,
    request: IdRequest,
) -> CommandResult<OrderDetail> {
    let mut detail = state
        .database
        .order_detail(&request.id)
        .await
        .map_err(crate::error::CommandError::from)?;
    resolve_detail_image(&state, &mut detail).map_err(crate::error::CommandError::from)?;
    Ok(detail)
}

#[tauri::command]
pub async fn update_order(
    state: State<'_, AppState>,
    request: UpdateOrderRequest,
) -> CommandResult<OrderDetail> {
    let mut detail = state
        .database
        .update_order(request)
        .await
        .map_err(crate::error::CommandError::from)?;
    resolve_detail_image(&state, &mut detail).map_err(crate::error::CommandError::from)?;
    Ok(detail)
}

#[tauri::command]
pub async fn delete_order(state: State<'_, AppState>, request: IdRequest) -> CommandResult<()> {
    delete_order_inner(&state, &request.id)
        .await
        .map_err(Into::into)
}

async fn delete_order_inner(state: &AppState, id: &str) -> AppResult<()> {
    let relative = state.database.image_relative_path(id).await?;
    let source = resolve_managed_path(&state.app_data_dir, &relative)?;
    let trash_dir = state.app_data_dir.join("trash");
    tokio::fs::create_dir_all(&trash_dir).await?;
    let extension = Path::new(&relative)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("img");
    let trash = trash_dir.join(format!("{id}.{extension}.deleting"));
    tokio::fs::rename(&source, &trash).await?;

    if let Err(error) = state.database.delete_order_record(id).await {
        if let Err(restore_error) = tokio::fs::rename(&trash, &source).await {
            return Err(AppError::Internal(format!(
                "删除数据库记录失败，且无法恢复图片：{error}；{restore_error}"
            )));
        }
        return Err(error);
    }
    if let Err(error) = tokio::fs::remove_file(&trash).await {
        tracing::warn!(order_id = id, error = %error, "failed to remove trashed order image");
    }
    Ok(())
}

async fn emit_task_update(app: &AppHandle, state: &AppState, task_id: &str) -> AppResult<()> {
    let task = state.database.task(task_id).await?;
    let batch = state.database.batch(&task.batch_id).await?;
    app.emit("import-task-updated", TaskUpdateEvent { task, batch })
        .map_err(|error| AppError::Internal(error.to_string()))
}

fn resolve_detail_image(state: &AppState, detail: &mut OrderDetail) -> AppResult<()> {
    detail.image_path = resolve_managed_path(&state.app_data_dir, &detail.image_path)?
        .to_string_lossy()
        .into_owned();
    Ok(())
}
