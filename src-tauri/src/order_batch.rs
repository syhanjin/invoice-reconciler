use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use tauri::{AppHandle, Runtime};

use crate::aliyun_batch::{AliyunBatchClient, RemoteBatch, terminal_status};
use crate::db::Database;
use crate::entities::order_recognition_batch;
use crate::error::{AppError, AppResult};
use crate::models::{OrderRecognitionProvider, OrderRecognitionSettings};
use crate::ocr::normalize_content;
use crate::queue::emit_update;
use crate::settings::SettingsStore;

const CREATE_RECOVERY_GRACE: Duration = Duration::from_secs(5 * 60);

#[derive(Clone)]
pub struct OrderBatchManager {
    database: Database,
    client: AliyunBatchClient,
    settings: SettingsStore,
    app_data_dir: PathBuf,
    polling: Arc<tokio::sync::Mutex<HashSet<String>>>,
}

impl OrderBatchManager {
    pub fn new(
        database: Database,
        client: AliyunBatchClient,
        settings: SettingsStore,
        app_data_dir: PathBuf,
    ) -> Self {
        Self {
            database,
            client,
            settings,
            app_data_dir,
            polling: Arc::new(tokio::sync::Mutex::new(HashSet::new())),
        }
    }

    pub async fn submit<R: Runtime>(&self, app: AppHandle<R>) -> AppResult<String> {
        let settings = self.settings.private().await.order_recognition();
        if !matches!(settings.provider, OrderRecognitionProvider::AliyunBatch) {
            return Err(AppError::InvalidInput(
                "当前订单识别模式不是阿里云百炼 Batch".into(),
            ));
        }
        if settings.api_key.is_empty() {
            return Err(AppError::InvalidInput("阿里云百炼 API Key 未配置".into()));
        }
        let (batch, tasks) = self.database.create_recognition_batch(&settings).await?;
        emit_update(&app, "batch_created", None, None, Some(&batch.id));
        let manager = self.clone();
        let id = batch.id.clone();
        let submitted_id = id.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(error) = manager.upload_and_run(&app, batch, tasks, settings).await {
                tracing::error!(batch_id = id, error = %error, "failed to submit aliyun batch");
                let checkpoint = manager.database.recognition_batch(&id).await.ok();
                if checkpoint.is_some_and(|batch| batch.input_file_id.is_some()) {
                    if let Err(error) = manager.poll(&app, &id).await {
                        tracing::error!(batch_id = id, error = %error, "failed to supervise submitted aliyun batch");
                    }
                } else {
                    let _ = manager
                        .database
                        .finish_recognition_batch(&id, "failed", Some(&error.to_string()))
                        .await;
                    emit_update(&app, "batch_failed", None, None, Some(&id));
                }
            }
        });
        Ok(submitted_id)
    }

    async fn upload_and_run<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        batch: order_recognition_batch::Model,
        tasks: Vec<crate::entities::order_import_pool::Model>,
        settings: OrderRecognitionSettings,
    ) -> AppResult<()> {
        let jsonl = self
            .client
            .build_jsonl(&self.app_data_dir, &tasks, &settings)
            .await?;
        for (task_id, message) in jsonl.rejected {
            self.database
                .fail_recognition_batch_task(&task_id, &batch.id, "batch_input_invalid", &message)
                .await?;
            emit_update(app, "task_failed", Some(&task_id), None, Some(&batch.id));
        }
        if jsonl.bytes.is_empty() {
            return Err(AppError::InvalidInput(
                "没有符合百炼批处理限制的待提交任务".into(),
            ));
        }
        let input_file_id = self.client.upload(&settings, jsonl.bytes).await?;
        self.database
            .set_recognition_batch_input_file(&batch.id, &input_file_id)
            .await?;
        self.database
            .mark_recognition_batch_create_pending(&batch.id)
            .await?;
        let remote = self
            .client
            .create(&settings, &input_file_id, &batch.id)
            .await?;
        self.database
            .set_recognition_batch_submitted(&batch.id, &remote.id)
            .await?;
        emit_update(app, "batch_submitted", None, None, Some(&batch.id));
        self.poll(app, &batch.id).await
    }

    pub async fn resume<R: Runtime>(&self, app: AppHandle<R>) -> AppResult<()> {
        for batch in self.database.active_recognition_batches().await? {
            let manager = self.clone();
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = manager.poll(&handle, &batch.id).await {
                    tracing::error!(batch_id = batch.id, error = %error, "failed to supervise resumed aliyun batch");
                }
            });
        }
        Ok(())
    }

    async fn recover_remote_batch(
        &self,
        batch: order_recognition_batch::Model,
        api_key: &str,
    ) -> AppResult<order_recognition_batch::Model> {
        if batch.remote_batch_id.is_some() {
            return Ok(batch);
        }
        let Some(input_file_id) = batch.input_file_id.as_deref() else {
            self.database
                .finish_recognition_batch(
                    &batch.id,
                    "failed",
                    Some("应用退出时批次文件尚未上传完成，可重试"),
                )
                .await?;
            return Err(AppError::InvalidInput("批次没有可恢复的上传文件".into()));
        };

        // The create response may have been lost after Aliyun accepted it. Querying
        // by the persisted file ID and unique local name prevents duplicate jobs.
        let remote = self
            .client
            .find_by_input_file(&batch.base_url, api_key, input_file_id, &batch.id)
            .await?;
        let remote = match remote {
            Some(remote) => remote,
            None if batch.status == "uploading" || create_recovery_grace_elapsed(&batch) => {
                let settings = OrderRecognitionSettings {
                    provider: OrderRecognitionProvider::AliyunBatch,
                    base_url: batch.base_url.clone(),
                    model: batch.model.clone(),
                    api_key: api_key.to_owned(),
                    max_concurrency: 1,
                    enable_thinking: batch.enable_thinking,
                };
                self.client
                    .create(&settings, input_file_id, &batch.id)
                    .await?
            }
            None => {
                return Err(AppError::AliyunBatch(
                    "远端批次创建结果尚不可见，将继续按已上传文件恢复".into(),
                ));
            }
        };
        self.database
            .set_recognition_batch_submitted(&batch.id, &remote.id)
            .await?;
        self.database.recognition_batch(&batch.id).await
    }

    pub async fn cancel<R: Runtime>(&self, app: AppHandle<R>, id: &str) -> AppResult<()> {
        let batch = self.database.recognition_batch(id).await?;
        if matches!(batch.status.as_str(), "completed" | "failed" | "canceled") {
            return Err(AppError::InvalidInput("批次已经结束".into()));
        }
        let remote_id = batch
            .remote_batch_id
            .as_deref()
            .ok_or_else(|| AppError::InvalidInput("批次尚未提交到云端".into()))?;
        let api_key = self.settings.private().await.aliyun_api_key;
        if api_key.is_empty() {
            return Err(AppError::InvalidInput("阿里云百炼 API Key 未配置".into()));
        }
        let remote = self
            .client
            .cancel(&batch.base_url, &api_key, remote_id)
            .await?;
        self.database.mark_recognition_batch_canceling(id).await?;
        self.database
            .update_recognition_batch_remote(
                id,
                "canceling",
                remote.output_file_id.as_deref(),
                remote.error_file_id.as_deref(),
            )
            .await?;
        if terminal_status(&remote.status) {
            self.finalize(&app, batch, remote, &api_key).await?;
        } else {
            emit_update(&app, "batch_canceling", None, None, Some(id));
            let manager = self.clone();
            let handle = app.clone();
            let local_id = id.to_owned();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = manager.poll(&handle, &local_id).await {
                    tracing::error!(batch_id = local_id, error = %error, "failed to poll canceling aliyun batch");
                }
            });
        }
        Ok(())
    }

    async fn poll<R: Runtime>(&self, app: &AppHandle<R>, local_id: &str) -> AppResult<()> {
        {
            let mut polling = self.polling.lock().await;
            if !polling.insert(local_id.to_owned()) {
                return Ok(());
            }
        }
        let result = self.poll_inner(app, local_id).await;
        self.polling.lock().await.remove(local_id);
        result
    }

    async fn poll_inner<R: Runtime>(&self, app: &AppHandle<R>, local_id: &str) -> AppResult<()> {
        let mut interval = Duration::from_secs(3);
        loop {
            let mut batch = self.database.recognition_batch(local_id).await?;
            if matches!(batch.status.as_str(), "completed" | "failed" | "canceled") {
                return Ok(());
            }
            let api_key = self.settings.private().await.aliyun_api_key;
            if api_key.is_empty() {
                tracing::warn!(
                    batch_id = local_id,
                    "aliyun API key is missing while polling batch"
                );
                tokio::time::sleep(interval).await;
                interval = (interval * 2).min(Duration::from_secs(60));
                continue;
            }
            if batch.remote_batch_id.is_none() {
                match self.recover_remote_batch(batch, &api_key).await {
                    Ok(recovered) => batch = recovered,
                    Err(error) => {
                        let terminal =
                            self.database
                                .recognition_batch(local_id)
                                .await
                                .is_ok_and(|current| {
                                    matches!(current.status.as_str(), "failed" | "canceled")
                                });
                        if terminal {
                            return Ok(());
                        }
                        tracing::warn!(batch_id = local_id, error = %error, "temporary failure recovering aliyun batch");
                        emit_update(app, "batch_poll_retry", None, None, Some(local_id));
                        tokio::time::sleep(interval).await;
                        interval = (interval * 2).min(Duration::from_secs(60));
                        continue;
                    }
                }
            }
            let Some(remote_id) = batch.remote_batch_id.as_deref() else {
                continue;
            };
            let remote = match self
                .client
                .retrieve(&batch.base_url, &api_key, remote_id)
                .await
            {
                Ok(remote) => remote,
                Err(error) => {
                    tracing::warn!(batch_id = local_id, error = %error, "temporary failure polling aliyun batch");
                    emit_update(app, "batch_poll_retry", None, None, Some(local_id));
                    tokio::time::sleep(interval).await;
                    interval = (interval * 2).min(Duration::from_secs(60));
                    continue;
                }
            };
            let next_status = if batch.status == "canceling" && !terminal_status(&remote.status) {
                "canceling"
            } else {
                normalize_status(&remote.status)
            };
            self.database
                .update_recognition_batch_remote(
                    local_id,
                    next_status,
                    remote.output_file_id.as_deref(),
                    remote.error_file_id.as_deref(),
                )
                .await?;
            emit_update(app, "batch_updated", None, None, Some(local_id));
            if terminal_status(&remote.status) {
                match self.finalize(app, batch, remote, &api_key).await {
                    Ok(()) => return Ok(()),
                    Err(error) => {
                        tracing::warn!(batch_id = local_id, error = %error, "temporary failure finalizing aliyun batch");
                        emit_update(app, "batch_poll_retry", None, None, Some(local_id));
                        tokio::time::sleep(interval).await;
                        interval = (interval * 2).min(Duration::from_secs(60));
                        continue;
                    }
                }
            }
            interval = Duration::from_secs(3);
            tokio::time::sleep(interval).await;
            interval = (interval * 2).min(Duration::from_secs(30));
        }
    }

    async fn finalize<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        batch: order_recognition_batch::Model,
        remote: RemoteBatch,
        api_key: &str,
    ) -> AppResult<()> {
        if let Some(file_id) = remote.output_file_id.as_deref() {
            let content = self
                .client
                .file_content(&batch.base_url, api_key, file_id)
                .await?;
            for line in content.lines().filter(|line| !line.trim().is_empty()) {
                match serde_json::from_str::<BatchOutputLine>(line) {
                    Ok(row) => {
                        if !matches!(row.response.status_code, None | Some(200..=299)) {
                            self.database
                                .fail_recognition_batch_task(
                                    &row.custom_id,
                                    &batch.id,
                                    "aliyun_batch_item_error",
                                    &format!(
                                        "百炼批次请求返回 HTTP {}",
                                        row.response.status_code.unwrap_or_default()
                                    ),
                                )
                                .await?;
                            continue;
                        }
                        let Some(content) = row
                            .response
                            .body
                            .and_then(|body| body.choices.into_iter().next())
                            .and_then(|choice| choice.message.content)
                        else {
                            self.database
                                .fail_recognition_batch_task(
                                    &row.custom_id,
                                    &batch.id,
                                    "invalid_ocr",
                                    "百炼批次结果没有返回内容",
                                )
                                .await?;
                            continue;
                        };
                        match normalize_content(&content) {
                            Ok(normalized) => {
                                let inserted = self
                                    .database
                                    .persist_pool_order(
                                        &row.custom_id,
                                        &normalized,
                                        "aliyun_batch",
                                        &batch.model,
                                        Some(&batch.short_number),
                                        Some(&batch.id),
                                    )
                                    .await?;
                                if inserted {
                                    emit_update(
                                        app,
                                        "task_completed",
                                        Some(&row.custom_id),
                                        Some(&row.custom_id),
                                        Some(&batch.id),
                                    );
                                }
                            }
                            Err(error) => {
                                self.database
                                    .fail_recognition_batch_task(
                                        &row.custom_id,
                                        &batch.id,
                                        error.code(),
                                        &error.to_string(),
                                    )
                                    .await?;
                            }
                        }
                    }
                    Err(error) => {
                        tracing::warn!(batch_id = batch.id, error = %error, "invalid aliyun batch output line")
                    }
                }
            }
        }
        if let Some(file_id) = remote.error_file_id.as_deref() {
            let content = self
                .client
                .file_content(&batch.base_url, api_key, file_id)
                .await?;
            for line in content.lines().filter(|line| !line.trim().is_empty()) {
                if let Ok(row) = serde_json::from_str::<BatchErrorLine>(line) {
                    let message = row
                        .error
                        .and_then(|error| error.message)
                        .unwrap_or_else(|| "百炼批次中的请求失败".into());
                    let _ = self
                        .database
                        .fail_recognition_batch_task(
                            &row.custom_id,
                            &batch.id,
                            "aliyun_batch_item_error",
                            &message,
                        )
                        .await;
                }
            }
        }
        let final_status = if matches!(remote.status.as_str(), "cancelled" | "canceled") {
            "canceled"
        } else if remote.status == "completed" {
            "completed"
        } else {
            "failed"
        };
        let detail = remote.errors.as_ref().map(ToString::to_string).or_else(|| {
            (final_status == "canceled").then(|| "批次已取消，未完成任务可重试".into())
        });
        self.database
            .finish_recognition_batch(&batch.id, final_status, detail.as_deref())
            .await?;
        emit_update(app, "batch_completed", None, None, Some(&batch.id));
        Ok(())
    }
}

fn normalize_status(status: &str) -> &str {
    match status {
        "validating" => "validating",
        "in_progress" | "running" => "in_progress",
        "finalizing" => "finalizing",
        "completed" => "finalizing",
        "canceling" | "cancelling" => "canceling",
        "cancelled" | "canceled" => "canceling",
        "failed" | "expired" => "finalizing",
        _ => "submitted",
    }
}

fn create_recovery_grace_elapsed(batch: &order_recognition_batch::Model) -> bool {
    chrono::DateTime::parse_from_rfc3339(&batch.created_at).is_ok_and(|created| {
        chrono::Utc::now()
            .signed_duration_since(created)
            .num_seconds()
            >= CREATE_RECOVERY_GRACE.as_secs() as i64
    })
}

#[derive(Deserialize)]
struct BatchOutputLine {
    custom_id: String,
    response: BatchOutputResponse,
}

#[derive(Deserialize)]
struct BatchOutputResponse {
    status_code: Option<u16>,
    body: Option<BatchOutputBody>,
}

#[derive(Deserialize)]
struct BatchOutputBody {
    choices: Vec<BatchChoice>,
}

#[derive(Deserialize)]
struct BatchChoice {
    message: BatchMessage,
}

#[derive(Deserialize)]
struct BatchMessage {
    content: Option<String>,
}

#[derive(Deserialize)]
struct BatchErrorLine {
    custom_id: String,
    error: Option<BatchError>,
}

#[derive(Deserialize)]
struct BatchError {
    message: Option<String>,
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::future::IntoFuture;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::models::UpdateSettingsRequest;
    use axum::{
        Json, Router,
        extract::Query,
        routing::{get, post},
    };
    use serde_json::json;

    async fn settings_store_with_aliyun_key(
        directory: &std::path::Path,
        database: &Database,
        base_url: &str,
    ) -> SettingsStore {
        let legacy = database.settings().await.unwrap();
        let store = SettingsStore::load_or_migrate(directory, legacy)
            .await
            .unwrap();
        store
            .save(UpdateSettingsRequest {
                order_recognition_provider: OrderRecognitionProvider::AliyunBatch,
                ollama_base_url: "http://127.0.0.1:11434".into(),
                ollama_model: "qwen".into(),
                openai_base_url: "https://example.com/v1".into(),
                openai_model: "vision".into(),
                openai_api_key: None,
                clear_openai_api_key: false,
                aliyun_base_url: base_url.into(),
                aliyun_model: "qwen3-vl-flash".into(),
                aliyun_api_key: Some("secret".into()),
                clear_aliyun_api_key: false,
                aliyun_enable_thinking: false,
                max_concurrency: 3,
                tencent_secret_id: None,
                tencent_secret_key: None,
                clear_tencent_credentials: false,
                invoice_ocr_concurrency: 3,
                match_max_orders: 10,
                match_max_candidates: 100,
            })
            .await
            .unwrap();
        store
    }

    #[tokio::test]
    async fn restart_relinks_an_accepted_remote_batch_from_its_input_file() {
        let calls = Arc::new(AtomicUsize::new(0));
        let route_calls = calls.clone();
        let router = Router::new().route(
            "/v1/batches",
            get(move |Query(query): Query<HashMap<String, String>>| {
                let route_calls = route_calls.clone();
                async move {
                    route_calls.fetch_add(1, Ordering::SeqCst);
                    assert_eq!(
                        query.get("input_file_ids").map(String::as_str),
                        Some("file-1")
                    );
                    let recovery_name = query.get("ds_name").cloned().unwrap();
                    Json(json!({
                        "data": [{
                            "id": "remote-batch-1",
                            "status": "in_progress",
                            "input_file_id": "file-1",
                            "output_file_id": null,
                            "error_file_id": null,
                            "errors": null,
                            "metadata": { "ds_name": recovery_name }
                        }]
                    }))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(axum::serve(listener, router).into_future());

        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("restart.sqlite3");
        let database = Database::connect(&database_path).await.unwrap();
        database
            .create_pool_task("task-1", "order.png", "images/task-1.png", "hash-1")
            .await
            .unwrap();
        let settings = OrderRecognitionSettings {
            provider: OrderRecognitionProvider::AliyunBatch,
            base_url: format!("http://{address}/v1"),
            model: "qwen3-vl-flash".into(),
            api_key: "secret".into(),
            max_concurrency: 3,
            enable_thinking: false,
        };
        let (batch, _) = database.create_recognition_batch(&settings).await.unwrap();
        database
            .set_recognition_batch_input_file(&batch.id, "file-1")
            .await
            .unwrap();
        database.connection.clone().close().await.unwrap();

        let reopened = Database::connect(&database_path).await.unwrap();
        let legacy = reopened.settings().await.unwrap();
        let store = SettingsStore::load_or_migrate(directory.path(), legacy)
            .await
            .unwrap();
        let manager = OrderBatchManager::new(
            reopened.clone(),
            AliyunBatchClient::new().unwrap(),
            store,
            directory.path().to_path_buf(),
        );
        let checkpoint = reopened.recognition_batch(&batch.id).await.unwrap();

        let recovered = manager
            .recover_remote_batch(checkpoint, "secret")
            .await
            .unwrap();

        assert_eq!(recovered.remote_batch_id.as_deref(), Some("remote-batch-1"));
        assert_eq!(
            reopened.pool_task("task-1").await.unwrap().status,
            "batch_locked"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn uncertain_create_response_does_not_duplicate_the_remote_batch() {
        let list_calls = Arc::new(AtomicUsize::new(0));
        let create_calls = Arc::new(AtomicUsize::new(0));
        let get_calls = list_calls.clone();
        let post_calls = create_calls.clone();
        let router = Router::new().route(
            "/v1/batches",
            get(move || {
                let get_calls = get_calls.clone();
                async move {
                    get_calls.fetch_add(1, Ordering::SeqCst);
                    Json(json!({ "data": [] }))
                }
            })
            .post(move || {
                let post_calls = post_calls.clone();
                async move {
                    post_calls.fetch_add(1, Ordering::SeqCst);
                    Json(json!({
                        "id": "unexpected-duplicate",
                        "status": "validating",
                        "input_file_id": "file-1",
                        "output_file_id": null,
                        "error_file_id": null,
                        "errors": null,
                        "metadata": null
                    }))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(axum::serve(listener, router).into_future());

        let directory = tempfile::tempdir().unwrap();
        let database = Database::in_memory().await.unwrap();
        database
            .create_pool_task("task-1", "order.png", "images/task-1.png", "hash-1")
            .await
            .unwrap();
        let settings = OrderRecognitionSettings {
            provider: OrderRecognitionProvider::AliyunBatch,
            base_url: format!("http://{address}/v1"),
            model: "qwen3-vl-flash".into(),
            api_key: "secret".into(),
            max_concurrency: 3,
            enable_thinking: false,
        };
        let (batch, _) = database.create_recognition_batch(&settings).await.unwrap();
        database
            .set_recognition_batch_input_file(&batch.id, "file-1")
            .await
            .unwrap();
        database
            .mark_recognition_batch_create_pending(&batch.id)
            .await
            .unwrap();
        let store = settings_store_with_aliyun_key(
            directory.path(),
            &database,
            &format!("http://{address}/v1"),
        )
        .await;
        let manager = OrderBatchManager::new(
            database.clone(),
            AliyunBatchClient::new().unwrap(),
            store,
            directory.path().to_path_buf(),
        );

        let checkpoint = database.recognition_batch(&batch.id).await.unwrap();
        assert!(
            manager
                .recover_remote_batch(checkpoint, "secret")
                .await
                .is_err()
        );
        assert_eq!(list_calls.load(Ordering::SeqCst), 1);
        assert_eq!(create_calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            database.recognition_batch(&batch.id).await.unwrap().status,
            "submitted"
        );
    }

    #[tokio::test]
    async fn restart_releases_a_batch_that_never_uploaded() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::in_memory().await.unwrap();
        database
            .create_pool_task("task-1", "order.png", "images/task-1.png", "hash-1")
            .await
            .unwrap();
        let settings = OrderRecognitionSettings {
            provider: OrderRecognitionProvider::AliyunBatch,
            base_url: "https://example.com/v1".into(),
            model: "qwen3-vl-flash".into(),
            api_key: "secret".into(),
            max_concurrency: 3,
            enable_thinking: false,
        };
        let (batch, _) = database.create_recognition_batch(&settings).await.unwrap();
        let legacy = database.settings().await.unwrap();
        let store = SettingsStore::load_or_migrate(directory.path(), legacy)
            .await
            .unwrap();
        let manager = OrderBatchManager::new(
            database.clone(),
            AliyunBatchClient::new().unwrap(),
            store,
            directory.path().to_path_buf(),
        );

        assert!(
            manager
                .recover_remote_batch(batch.clone(), "secret")
                .await
                .is_err()
        );
        assert_eq!(
            database.recognition_batch(&batch.id).await.unwrap().status,
            "failed"
        );
        let task = database.pool_task("task-1").await.unwrap();
        assert_eq!(task.status, "failed");
        assert!(task.recognition_batch_id.is_none());
    }

    #[tokio::test]
    async fn cancel_after_restart_releases_locked_tasks_without_another_restart() {
        let router = Router::new()
            .route(
                "/v1/batches/remote-batch-1/cancel",
                post(|| async {
                    Json(json!({
                        "id": "remote-batch-1",
                        "status": "cancelling",
                        "input_file_id": "file-1",
                        "output_file_id": null,
                        "error_file_id": null,
                        "errors": null,
                        "metadata": null
                    }))
                }),
            )
            .route(
                "/v1/batches/remote-batch-1",
                get(|| async {
                    Json(json!({
                        "id": "remote-batch-1",
                        "status": "cancelled",
                        "input_file_id": "file-1",
                        "output_file_id": null,
                        "error_file_id": null,
                        "errors": null,
                        "metadata": null
                    }))
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(axum::serve(listener, router).into_future());

        let directory = tempfile::tempdir().unwrap();
        let database = Database::in_memory().await.unwrap();
        database
            .create_pool_task("task-1", "order.png", "images/task-1.png", "hash-1")
            .await
            .unwrap();
        let base_url = format!("http://{address}/v1");
        let batch_settings = OrderRecognitionSettings {
            provider: OrderRecognitionProvider::AliyunBatch,
            base_url: base_url.clone(),
            model: "qwen3-vl-flash".into(),
            api_key: "secret".into(),
            max_concurrency: 3,
            enable_thinking: false,
        };
        let (batch, _) = database
            .create_recognition_batch(&batch_settings)
            .await
            .unwrap();
        database
            .set_recognition_batch_input_file(&batch.id, "file-1")
            .await
            .unwrap();
        database
            .set_recognition_batch_submitted(&batch.id, "remote-batch-1")
            .await
            .unwrap();
        let store = settings_store_with_aliyun_key(directory.path(), &database, &base_url).await;
        let manager = OrderBatchManager::new(
            database.clone(),
            AliyunBatchClient::new().unwrap(),
            store,
            directory.path().to_path_buf(),
        );
        let app = tauri::test::mock_app();

        manager
            .cancel(app.handle().clone(), &batch.id)
            .await
            .unwrap();

        for _ in 0..50 {
            if database.recognition_batch(&batch.id).await.unwrap().status == "canceled" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        let released = database.pool_task("task-1").await.unwrap();
        assert_eq!(released.status, "failed");
        assert!(released.recognition_batch_id.is_none());
        assert_eq!(
            database.recognition_batch(&batch.id).await.unwrap().status,
            "canceled"
        );
    }
}
