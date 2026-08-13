use std::collections::HashSet;
use std::path::{Path, PathBuf};

use tauri::{AppHandle, State};
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult, CommandResult};
use crate::files::{copy_managed_image, normalized_extension, resolve_managed_path, sha256_file};
use crate::models::{
    AppSettings, BootstrapState, DeleteOrderResult, DeleteOrdersRequest, DeleteOrdersResult,
    IdRequest, ImportOrderImagesRequest, ImportOrderImagesResult, LlmProvider, LlmStatus,
    OrderDetail, OrderImportLogsPage, OrderImportLogsQuery, OrderImportPool, OrdersPage,
    OrdersQuery, UpdateOrderRequest, UpdateSettingsRequest,
};
use crate::queue::emit_update;

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
            settings: state.settings.public().await,
            latest_invoice_batch: state.database.latest_invoice_batch().await?,
        })
    }
    .await
    .map_err(Into::into)
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> CommandResult<AppSettings> {
    Ok(state.settings.public().await)
}

#[tauri::command]
pub async fn save_settings(
    state: State<'_, AppState>,
    request: UpdateSettingsRequest,
) -> CommandResult<AppSettings> {
    let settings = state
        .settings
        .save(request)
        .await
        .map_err(crate::error::CommandError::from)?;
    state.queue.wake();
    Ok(settings)
}

#[tauri::command]
pub async fn check_llm(
    state: State<'_, AppState>,
    request: UpdateSettingsRequest,
) -> CommandResult<LlmStatus> {
    async {
        let public = request.clone().validate()?;
        let current = state.settings.private().await;
        let settings = crate::models::LlmSettings {
            provider: public.llm_provider,
            base_url: match public.llm_provider {
                LlmProvider::Ollama => public.ollama_base_url,
                LlmProvider::Openai => public.openai_base_url,
            },
            model: match public.llm_provider {
                LlmProvider::Ollama => public.ollama_model,
                LlmProvider::Openai => public.openai_model,
            },
            api_key: match public.llm_provider {
                LlmProvider::Ollama => String::new(),
                LlmProvider::Openai => request
                    .openai_api_key
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned)
                    .unwrap_or(current.openai_api_key),
            },
        };
        state.llm.check(&settings).await
    }
    .await
    .map_err(Into::into)
}

#[tauri::command]
pub async fn import_order_images(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ImportOrderImagesRequest,
) -> CommandResult<ImportOrderImagesResult> {
    import_order_images_inner(&app, &state, request)
        .await
        .map_err(Into::into)
}

async fn import_order_images_inner(
    app: &AppHandle,
    state: &AppState,
    request: ImportOrderImagesRequest,
) -> AppResult<ImportOrderImagesResult> {
    if request.image_paths.is_empty() {
        return Err(AppError::InvalidInput("请至少选择一张订单截图".into()));
    }
    if request.image_paths.len() > 500 {
        return Err(AppError::InvalidInput("单次最多导入 500 张图片".into()));
    }
    let _platform = request.platform;

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

    let mut imported = 0;
    let mut skipped = 0;
    let mut copied_paths = Vec::new();
    let mut pool_tasks = Vec::new();
    let creation_result: AppResult<()> = async {
        for image in prepared {
            let id = Uuid::now_v7();
            if image.duplicate {
                skipped += 1;
                continue;
            }

            let relative =
                copy_managed_image(&image.source, &state.images_dir, id, &image.extension).await?;
            let relative_string = relative.to_string_lossy().replace('\\', "/");
            copied_paths.push(state.app_data_dir.join(&relative));
            pool_tasks.push((
                id.to_string(),
                image.original_file_name,
                relative_string,
                image.hash,
            ));
            imported += 1;
        }
        state.database.create_pool_tasks(&pool_tasks).await?;
        Ok(())
    }
    .await;

    if let Err(error) = creation_result {
        for path in copied_paths {
            let _ = tokio::fs::remove_file(path).await;
        }
        return Err(error);
    }
    emit_update(app, "tasks_imported", None, None);
    state.queue.wake();
    Ok(ImportOrderImagesResult { imported, skipped })
}

#[tauri::command]
pub async fn get_order_import_pool(state: State<'_, AppState>) -> CommandResult<OrderImportPool> {
    state.database.order_import_pool().await.map_err(Into::into)
}

#[tauri::command]
pub async fn list_order_import_logs(
    state: State<'_, AppState>,
    query: OrderImportLogsQuery,
) -> CommandResult<OrderImportLogsPage> {
    state
        .database
        .order_import_logs(query)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn retry_order_import_task(
    app: AppHandle,
    state: State<'_, AppState>,
    request: IdRequest,
) -> CommandResult<bool> {
    let retried = state
        .database
        .retry_pool_task(&request.id)
        .await
        .map_err(crate::error::CommandError::from)?;
    if retried {
        emit_update(&app, "task_retried", Some(&request.id), None);
        state.queue.wake();
    }
    Ok(retried)
}

#[tauri::command]
pub async fn retry_failed_order_import_tasks(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<u64> {
    let affected = state
        .database
        .retry_all_pool_tasks()
        .await
        .map_err(crate::error::CommandError::from)?;
    if affected > 0 {
        emit_update(&app, "tasks_retried", None, None);
        state.queue.wake();
    }
    Ok(affected)
}

#[tauri::command]
pub async fn delete_order_import_task(
    app: AppHandle,
    state: State<'_, AppState>,
    request: IdRequest,
) -> CommandResult<()> {
    delete_order_import_task_inner(&state, &request.id)
        .await
        .map_err(crate::error::CommandError::from)?;
    emit_update(&app, "task_deleted", Some(&request.id), None);
    Ok(())
}

#[tauri::command]
pub async fn list_orders(
    state: State<'_, AppState>,
    query: OrdersQuery,
) -> CommandResult<OrdersPage> {
    let settings = state.settings.private().await;
    state
        .database
        .list_orders_with_matching(
            query,
            usize::from(settings.match_max_orders),
            usize::from(settings.match_max_candidates),
        )
        .await
        .map_err(Into::into)
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
    state.match_summary_queue.wake();
    resolve_detail_image(&state, &mut detail).map_err(crate::error::CommandError::from)?;
    Ok(detail)
}

#[tauri::command]
pub async fn delete_order(state: State<'_, AppState>, request: IdRequest) -> CommandResult<()> {
    delete_order_inner(&state, &request.id)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn delete_orders(
    state: State<'_, AppState>,
    request: DeleteOrdersRequest,
) -> CommandResult<DeleteOrdersResult> {
    delete_orders_inner(&state, request)
        .await
        .map_err(Into::into)
}

async fn delete_orders_inner(
    state: &AppState,
    request: DeleteOrdersRequest,
) -> AppResult<DeleteOrdersResult> {
    if request.ids.is_empty() {
        return Err(AppError::InvalidInput("请至少选择一个订单".into()));
    }
    if request.ids.len() > 100 {
        return Err(AppError::InvalidInput("单次最多删除 100 个订单".into()));
    }

    let mut seen = HashSet::new();
    let mut results = Vec::with_capacity(request.ids.len());
    for id in request.ids {
        if !seen.insert(id.clone()) {
            continue;
        }
        match delete_order_inner(state, &id).await {
            Ok(()) => results.push(DeleteOrderResult {
                id,
                deleted: true,
                error_code: None,
                error_message: None,
            }),
            Err(error) => results.push(DeleteOrderResult {
                id,
                deleted: false,
                error_code: Some(error.code().to_owned()),
                error_message: Some(error.to_string()),
            }),
        }
    }
    Ok(DeleteOrdersResult { results })
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

async fn delete_order_import_task_inner(state: &AppState, id: &str) -> AppResult<()> {
    let task = state.database.pool_task(id).await?;
    if !matches!(task.status.as_str(), "pending" | "failed") {
        return Err(AppError::InvalidInput(
            "只有未锁定的待处理或失败任务可以删除".into(),
        ));
    }
    let source = resolve_managed_path(&state.app_data_dir, &task.managed_image_path)?;
    let trash_dir = state.app_data_dir.join("trash");
    tokio::fs::create_dir_all(&trash_dir).await?;
    let extension = Path::new(&task.managed_image_path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("img");
    let trash = trash_dir.join(format!("{}.{}.deleting", task.id, extension));
    tokio::fs::rename(&source, &trash).await?;
    if let Err(error) = state.database.delete_pool_task_record(id).await {
        if let Err(restore_error) = tokio::fs::rename(&trash, &source).await {
            return Err(AppError::Internal(format!(
                "删除任务记录失败，且无法恢复图片：{error}；{restore_error}"
            )));
        }
        return Err(error);
    }
    if let Err(error) = tokio::fs::remove_file(&trash).await {
        tracing::warn!(task_id = id, error = %error, "failed to remove deleted pool image");
    }
    Ok(())
}

fn resolve_detail_image(state: &AppState, detail: &mut OrderDetail) -> AppResult<()> {
    detail.image_path = resolve_managed_path(&state.app_data_dir, &detail.image_path)?
        .to_string_lossy()
        .into_owned();
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::db::Database;
    use crate::models::{NormalizedOrder, OrderImportLogsQuery};

    async fn test_state() -> (TempDir, AppState) {
        let directory = tempfile::tempdir().unwrap();
        let app_data_dir = directory.path().to_path_buf();
        let images_dir = app_data_dir.join("images");
        tokio::fs::create_dir_all(&images_dir).await.unwrap();
        let database = Database::in_memory().await.unwrap();
        let state = AppState::test(database, app_data_dir, images_dir).await;
        (directory, state)
    }

    async fn insert_order(state: &AppState, id: &str, number: &str) -> PathBuf {
        let relative = format!("images/{id}.png");
        let image = state.app_data_dir.join(&relative);
        tokio::fs::write(&image, b"managed image").await.unwrap();
        state
            .database
            .create_pool_task(id, "order.png", &relative, id)
            .await
            .unwrap();
        assert_eq!(
            state
                .database
                .claim_next_realtime_task()
                .await
                .unwrap()
                .unwrap()
                .id,
            id
        );
        state
            .database
            .persist_pool_order(
                id,
                &NormalizedOrder {
                    order_number: Some(number.into()),
                    product_names: vec!["商品".into()],
                    paid_amount_fen: Some(100),
                    ordered_at: None,
                    shop_name: None,
                    warning: None,
                    raw_json: "{}".into(),
                },
                "ollama",
                "test-model",
            )
            .await
            .unwrap();
        image
    }

    #[tokio::test]
    async fn bulk_delete_reports_partial_results_and_deduplicates_ids() {
        let (_directory, state) = test_state().await;
        let first_image = insert_order(&state, "order-a", "duplicate").await;
        let second_image = insert_order(&state, "order-b", "duplicate").await;

        let result = delete_orders_inner(
            &state,
            DeleteOrdersRequest {
                ids: vec!["order-a".into(), "missing".into(), "order-a".into()],
            },
        )
        .await
        .unwrap();

        assert_eq!(result.results.len(), 2);
        assert!(result.results[0].deleted);
        assert_eq!(result.results[0].id, "order-a");
        assert!(!first_image.exists());
        assert!(state.database.order_detail("order-a").await.is_err());
        assert!(!state.database.hash_exists("order-a").await.unwrap());
        let logs = state
            .database
            .order_import_logs(OrderImportLogsQuery::default())
            .await
            .unwrap();
        assert_eq!(logs.total, 2);
        assert!(logs.items.iter().any(|log| log.task_id == "order-a"));
        state
            .database
            .create_pool_task(
                "reimported-order-a",
                "order.png",
                "images/reimported-order-a.png",
                "order-a",
            )
            .await
            .unwrap();
        assert!(state.database.hash_exists("order-a").await.unwrap());
        assert!(!result.results[1].deleted);
        assert_eq!(result.results[1].id, "missing");
        assert_eq!(result.results[1].error_code.as_deref(), Some("not_found"));
        assert!(second_image.exists());
        assert!(
            !state
                .database
                .order_detail("order-b")
                .await
                .unwrap()
                .possible_duplicate
        );
    }

    #[tokio::test]
    async fn bulk_delete_restores_image_when_database_delete_fails() {
        let (_directory, state) = test_state().await;
        let image = insert_order(&state, "order-a", "10001").await;
        state
            .database
            .execute_test_statement(
                "CREATE TRIGGER fail_order_delete BEFORE DELETE ON orders \
                 WHEN OLD.id = 'order-a' \
                 BEGIN SELECT RAISE(FAIL, 'forced delete failure'); END",
            )
            .await
            .unwrap();

        let result = delete_orders_inner(
            &state,
            DeleteOrdersRequest {
                ids: vec!["order-a".into()],
            },
        )
        .await
        .unwrap();

        assert_eq!(result.results.len(), 1);
        assert!(!result.results[0].deleted);
        assert_eq!(
            result.results[0].error_code.as_deref(),
            Some("database_error")
        );
        assert!(image.exists());
        assert!(state.database.order_detail("order-a").await.is_ok());
        assert!(state.database.hash_exists("order-a").await.unwrap());
        assert_eq!(
            state
                .database
                .order_detail("order-a")
                .await
                .unwrap()
                .image_path,
            "images/order-a.png"
        );
    }

    #[tokio::test]
    async fn bulk_delete_validates_request_size() {
        let (_directory, state) = test_state().await;
        let empty = delete_orders_inner(&state, DeleteOrdersRequest { ids: vec![] }).await;
        assert!(matches!(empty, Err(AppError::InvalidInput(_))));

        let oversized = delete_orders_inner(
            &state,
            DeleteOrdersRequest {
                ids: (0..101).map(|index| format!("order-{index}")).collect(),
            },
        )
        .await;
        assert!(matches!(oversized, Err(AppError::InvalidInput(_))));
    }
}
