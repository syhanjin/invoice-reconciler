use std::path::PathBuf;
use std::sync::Arc;

use futures::{StreamExt, stream};
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;

use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::files::resolve_managed_path;
use crate::models::{AppSettings, Platform, TaskUpdateEvent};
use crate::ocr::OllamaClient;

#[derive(Clone)]
pub struct QueueManager {
    database: Database,
    ollama: OllamaClient,
    app_data_dir: PathBuf,
    active_batch: Arc<Mutex<Option<String>>>,
}

impl QueueManager {
    pub fn new(database: Database, ollama: OllamaClient, app_data_dir: PathBuf) -> Self {
        Self {
            database,
            ollama,
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
        settings: AppSettings,
    ) -> AppResult<()> {
        if task_ids.is_empty() {
            let result = self.database.finalize_batch(&batch_id).await;
            self.release(&batch_id).await;
            return result;
        }

        let queue = self.clone();
        tauri::async_runtime::spawn(async move {
            queue.run_batch(&app, &batch_id, task_ids, settings).await;
            queue.release(&batch_id).await;
        });
        Ok(())
    }

    async fn run_batch(
        &self,
        app: &AppHandle,
        batch_id: &str,
        task_ids: Vec<String>,
        settings: AppSettings,
    ) {
        let concurrency = usize::from(settings.max_concurrency);
        self.database.mark_batch_processing(batch_id).await.ok();
        run_limited(task_ids, concurrency, |task_id| {
            let settings = settings.clone();
            async move {
                self.process_one(app, &task_id, &settings).await;
            }
        })
        .await;
        self.database.finalize_batch(batch_id).await.ok();

        if let Ok(batch) = self.database.batch(batch_id).await
            && let Some(task) = batch.tasks.last().cloned()
        {
            let _ = app.emit("import-task-updated", TaskUpdateEvent { task, batch });
        }
    }

    async fn process_one(&self, app: &AppHandle, task_id: &str, settings: &AppSettings) {
        match self.database.mark_processing(task_id).await {
            Ok(true) => self.emit_update(app, task_id).await,
            Ok(false) => return,
            Err(error) => {
                tracing::error!(task_id, error = %error, "failed to start import task");
                return;
            }
        }

        let result = async {
            let task = self.database.task(task_id).await?;
            let relative = task
                .managed_image_path
                .as_deref()
                .ok_or_else(|| AppError::File("任务没有受管图片".into()))?;
            let image_path = resolve_managed_path(&self.app_data_dir, relative)?;
            let recognized = self.ollama.recognize(settings, &image_path).await?;
            self.database
                .persist_recognized_order(task_id, Platform::Taobao, &recognized)
                .await
        }
        .await;

        if let Err(error) = result {
            let code = error.code();
            let message = error.to_string();
            tracing::warn!(task_id, code, error = %message, "import task failed");
            let _ = self.database.mark_failed(task_id, code, &message).await;
        }
        self.emit_update(app, task_id).await;
    }

    async fn emit_update(&self, app: &AppHandle, task_id: &str) {
        let Ok(task) = self.database.task(task_id).await else {
            return;
        };
        let Ok(batch) = self.database.batch(&task.batch_id).await else {
            return;
        };
        let _ = app.emit("import-task-updated", TaskUpdateEvent { task, batch });
    }
}

async fn run_limited<T, F, Fut>(items: Vec<T>, concurrency: usize, worker: F)
where
    F: Fn(T) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    stream::iter(items)
        .for_each_concurrent(concurrency.max(1), worker)
        .await;
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[tokio::test]
    async fn limited_executor_never_exceeds_configured_concurrency() {
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        run_limited((0..20).collect(), 3, |_| {
            let active = active.clone();
            let maximum = maximum.clone();
            async move {
                let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                maximum.fetch_max(current, Ordering::SeqCst);
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                active.fetch_sub(1, Ordering::SeqCst);
            }
        })
        .await;
        assert_eq!(maximum.load(Ordering::SeqCst), 3);
    }
}
