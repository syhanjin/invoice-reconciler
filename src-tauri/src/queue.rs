use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures::{StreamExt, stream::FuturesUnordered};
use tauri::{AppHandle, Emitter, Runtime};
use tokio::sync::Notify;

use crate::db::Database;
use crate::files::resolve_managed_path;
use crate::models::{OrderImportUpdateEvent, OrderRecognitionSettings};
use crate::ocr::OllamaClient;
use crate::settings::SettingsStore;

#[derive(Clone)]
pub struct QueueManager {
    database: Database,
    recognizer: OllamaClient,
    settings: SettingsStore,
    app_data_dir: PathBuf,
    notify: Arc<Notify>,
    started: Arc<AtomicBool>,
}

impl QueueManager {
    pub fn new(
        database: Database,
        recognizer: OllamaClient,
        settings: SettingsStore,
        app_data_dir: PathBuf,
    ) -> Self {
        Self {
            database,
            recognizer,
            settings,
            app_data_dir,
            notify: Arc::new(Notify::new()),
            started: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn start(&self, app: AppHandle) {
        if self.started.swap(true, Ordering::AcqRel) {
            return;
        }
        let queue = self.clone();
        tauri::async_runtime::spawn(async move {
            queue.notify.notify_one();
            loop {
                queue.notify.notified().await;
                queue.drain(&app).await;
            }
        });
    }

    pub fn wake(&self) {
        self.notify.notify_one();
    }

    async fn drain(&self, app: &AppHandle) {
        let mut in_flight = FuturesUnordered::new();
        loop {
            let settings = self.settings.private().await.order_recognition();
            let concurrency = usize::from(settings.max_concurrency.max(1));
            while in_flight.len() < concurrency {
                match self.database.claim_next_realtime_task().await {
                    Ok(Some(task)) => {
                        in_flight.push(self.process(app, task.id, settings.clone()));
                    }
                    Ok(None) => break,
                    Err(error) => {
                        tracing::error!(error = %error, "failed to claim order import task");
                        break;
                    }
                }
            }
            if in_flight.is_empty() {
                return;
            }
            in_flight.next().await;
        }
    }

    async fn process(&self, app: &AppHandle, task_id: String, settings: OrderRecognitionSettings) {
        emit_update(app, "task_processing", Some(&task_id), None);
        let result = async {
            let task = self.database.pool_task(&task_id).await?;
            let image_path = resolve_managed_path(&self.app_data_dir, &task.managed_image_path)?;
            let normalized = self.recognizer.recognize(&settings, &image_path).await?;
            self.database
                .persist_pool_order(
                    &task_id,
                    &normalized,
                    settings.provider.as_str(),
                    &settings.model,
                )
                .await
                .map(|_| ())
        }
        .await;

        match result {
            Ok(()) => emit_update(app, "task_completed", Some(&task_id), Some(&task_id)),
            Err(error) => {
                tracing::warn!(task_id, code = error.code(), error = %error, "order recognition task failed");
                let _ = self
                    .database
                    .fail_pool_task(&task_id, error.code(), &error.to_string())
                    .await;
                emit_update(app, "task_failed", Some(&task_id), None);
            }
        }
    }
}

pub(crate) fn emit_update<R: Runtime>(
    app: &AppHandle<R>,
    kind: &str,
    task_id: Option<&str>,
    order_id: Option<&str>,
) {
    let _ = app.emit(
        "order-import-updated",
        OrderImportUpdateEvent {
            kind: kind.to_owned(),
            task_id: task_id.map(str::to_owned),
            order_id: order_id.map(str::to_owned),
        },
    );
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use futures::stream;

    use super::*;

    #[tokio::test]
    async fn limited_executor_never_exceeds_configured_concurrency() {
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        stream::iter(0..20)
            .for_each_concurrent(Some(3), |_| {
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
