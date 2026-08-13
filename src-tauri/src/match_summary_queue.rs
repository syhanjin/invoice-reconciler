use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Emitter, Runtime};
use tokio::sync::Notify;
use tokio::task::JoinSet;

use crate::db::Database;
use crate::models::MatchSummaryUpdatedEvent;
use crate::ocr::LlmClient;
use crate::settings::SettingsStore;

#[derive(Clone)]
pub struct MatchSummaryQueue {
    database: Database,
    llm: LlmClient,
    settings: SettingsStore,
    notify: Arc<Notify>,
    started: Arc<AtomicBool>,
}

impl MatchSummaryQueue {
    pub fn new(database: Database, llm: LlmClient, settings: SettingsStore) -> Self {
        Self {
            database,
            llm,
            settings,
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
        self.drain_with(|invoice_id, status| emit_update(app, invoice_id, status))
            .await;
    }

    async fn drain_with(&self, notify: impl Fn(&str, &str)) {
        if self.settings.private().await.summary_generation_mode
            == crate::models::SummaryGenerationMode::Automatic
            && let Err(error) = self.database.activate_automatic_match_summaries().await
        {
            tracing::error!(error = %error, "failed to activate automatic match summaries");
            return;
        }
        let mut running = JoinSet::new();
        loop {
            let concurrency = usize::from(
                self.settings
                    .private()
                    .await
                    .summary_generation_max_concurrency,
            );
            let mut claimed_jobs = Vec::new();
            for _ in 0..concurrency.saturating_sub(running.len()) {
                let settings = self.settings.private().await.llm();
                let claimed = match self
                    .database
                    .claim_next_match_summary_with_model(
                        settings.provider.as_str(),
                        &settings.model,
                    )
                    .await
                {
                    Ok(Some(claimed)) => claimed,
                    Ok(None) => break,
                    Err(error) => {
                        tracing::error!(error = %error, "failed to claim match summary");
                        break;
                    }
                };
                notify(&claimed.invoice_id, "processing");
                claimed_jobs.push((claimed, settings));
            }
            for (claimed, settings) in claimed_jobs {
                let queue = self.clone();
                running.spawn(async move { queue.process_one(claimed, settings).await });
            }
            let Some(result) = running.join_next().await else {
                return;
            };
            match result {
                Ok((invoice_id, Some(status))) => notify(&invoice_id, &status),
                Ok((_invoice_id, None)) => {}
                Err(error) => {
                    tracing::error!(error = %error, "match summary worker stopped unexpectedly")
                }
            }
        }
    }

    async fn process_one(
        &self,
        claimed: crate::models::ClaimedMatchSummary,
        settings: crate::models::LlmSettings,
    ) -> (String, Option<String>) {
        let invoice_id = claimed.invoice_id.clone();
        let result = self.llm.summarize_match(&settings, &claimed.input).await;
        match result {
            Ok(display_name) => {
                match self
                    .database
                    .complete_match_summary(
                        &claimed.invoice_id,
                        &claimed.input_hash,
                        &display_name,
                        settings.provider.as_str(),
                        &settings.model,
                    )
                    .await
                {
                    Ok(true) => (invoice_id, Some("succeeded".into())),
                    Ok(false) => {
                        tracing::info!(
                            attempt_id = claimed.attempt_id,
                            invoice_id,
                            "discarded stale match summary response"
                        );
                        (invoice_id, None)
                    }
                    Err(error) => {
                        tracing::error!(attempt_id = claimed.attempt_id, invoice_id, error = %error, "failed to persist match summary");
                        (invoice_id, None)
                    }
                }
            }
            Err(error) => {
                tracing::warn!(attempt_id = claimed.attempt_id, invoice_id, error = %error, "match summary generation failed");
                let status = self
                    .database
                    .fail_match_summary(
                        &claimed.invoice_id,
                        &claimed.input_hash,
                        &error.to_string(),
                        settings.provider.as_str(),
                        &settings.model,
                    )
                    .await
                    .unwrap_or(None);
                (invoice_id, status)
            }
        }
    }
}

pub(crate) fn emit_update<R: Runtime>(app: &AppHandle<R>, invoice_id: &str, status: &str) {
    let _ = app.emit(
        "match-summary-updated",
        MatchSummaryUpdatedEvent {
            invoice_id: invoice_id.to_owned(),
            status: status.to_owned(),
        },
    );
}

#[cfg(test)]
mod tests {
    use std::future::IntoFuture;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use axum::{Json, Router, routing::post};
    use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};
    use serde_json::{Value, json};

    use super::*;
    use crate::models::{
        AppSettings, DEFAULT_INVOICE_OCR_CONCURRENCY, DEFAULT_MATCH_MAX_CANDIDATES,
        DEFAULT_MATCH_MAX_ORDERS, DEFAULT_OPENAI_BASE_URL, DEFAULT_OPENAI_MODEL, LlmProvider,
    };

    fn settings(base_url: String) -> AppSettings {
        AppSettings {
            llm_provider: LlmProvider::Ollama,
            ollama_base_url: base_url,
            ollama_model: "summary-model".into(),
            openai_base_url: DEFAULT_OPENAI_BASE_URL.into(),
            openai_model: DEFAULT_OPENAI_MODEL.into(),
            openai_api_key_masked: None,
            openai_api_key_configured: false,
            order_recognition_max_concurrency: 8,
            summary_generation_mode: crate::models::SummaryGenerationMode::Automatic,
            summary_generation_max_concurrency: 2,
            tencent_secret_id_masked: None,
            tencent_credentials_configured: false,
            invoice_ocr_concurrency: DEFAULT_INVOICE_OCR_CONCURRENCY,
            match_max_orders: DEFAULT_MATCH_MAX_ORDERS,
            match_max_candidates: DEFAULT_MATCH_MAX_CANDIDATES,
            settings_path: String::new(),
            permission_warning: None,
        }
    }

    #[tokio::test]
    async fn honors_configured_match_summary_concurrency() {
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let calls = Arc::new(AtomicUsize::new(0));
        let router = Router::new().route(
            "/api/chat",
            post({
                let active = active.clone();
                let peak = peak.clone();
                let calls = calls.clone();
                move |Json(_body): Json<Value>| {
                    let active = active.clone();
                    let peak = peak.clone();
                    let calls = calls.clone();
                    async move {
                        let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                        peak.fetch_max(current, Ordering::SeqCst);
                        calls.fetch_add(1, Ordering::SeqCst);
                        tokio::time::sleep(Duration::from_millis(30)).await;
                        active.fetch_sub(1, Ordering::SeqCst);
                        Json(json!({"message": {"content": "{\"display_name\":\"测试商品\"}"}}))
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(axum::serve(listener, router).into_future());

        let database = Database::in_memory().await.unwrap();
        database.execute_test_statement(
            "INSERT INTO import_batches (id,platform,status,created_at) VALUES ('ob','taobao','completed','now');
             INSERT INTO import_tasks (id,batch_id,original_file_name,image_sha256,status,retry_count,created_at) VALUES ('o1','ob','o1.png','oh1','succeeded',0,'now'),('o2','ob','o2.png','oh2','succeeded',0,'now');
             INSERT INTO orders (id,platform,external_order_number,paid_amount_fen,shop_name,raw_ocr_json,created_at,updated_at) VALUES ('o1','taobao','ORDER-1',1000,'店铺','{}','now','now'),('o2','taobao','ORDER-2',1000,'店铺','{}','now','now');
             INSERT INTO order_items (order_id,position,product_name) VALUES ('o1',0,'商品一'),('o2',0,'商品二');
             INSERT INTO invoice_batches (id,status,created_at) VALUES ('ib','completed','now');
             INSERT INTO invoice_tasks (id,batch_id,original_file_name,source_sha256,media_type,status,retry_count,created_at) VALUES ('it1','ib','i1.pdf','ih1','application/pdf','succeeded',0,'now'),('it2','ib','i2.pdf','ih2','application/pdf','succeeded',0,'now');
             INSERT INTO invoices (id,task_id,type,subtype,type_description,subtype_description,total_fen,currency,preview_path,raw_invoice_json,created_at,updated_at) VALUES ('i1','it1',3,'VatInvoice','发票','发票',1000,'CNY','i1.pdf','{}','now','now'),('i2','it2',3,'VatInvoice','发票','发票',1000,'CNY','i2.pdf','{}','now','now');
             INSERT INTO invoice_items (invoice_id,position,name,raw_json) VALUES ('i1',0,'商品一','{}'),('i2',0,'商品二','{}');
             INSERT INTO invoice_matches (invoice_id,source,confirmed_at) VALUES ('i1','automatic','now'),('i2','automatic','now');
             INSERT INTO invoice_match_orders (invoice_id,order_id,position) VALUES ('i1','o1',0),('i2','o2',0);",
        ).await.unwrap();
        let directory = tempfile::tempdir().unwrap();
        let store =
            SettingsStore::load_or_migrate(directory.path(), settings(format!("http://{address}")))
                .await
                .unwrap();
        let queue = MatchSummaryQueue::new(database.clone(), LlmClient::new().unwrap(), store);

        queue.drain_with(|_, _| {}).await;

        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(peak.load(Ordering::SeqCst), 2);
        let succeeded = crate::entities::invoice_match::Entity::find()
            .filter(crate::entities::invoice_match::Column::SummaryStatus.eq("succeeded"))
            .count(&database.connection)
            .await
            .unwrap();
        assert_eq!(succeeded, 2);
    }
}
