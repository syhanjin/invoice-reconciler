use std::path::Path;
use std::time::Duration;

use base64::Engine;
use reqwest::multipart::{Form, Part};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::entities::order_import_pool;
use crate::error::{AppError, AppResult};
use crate::models::OrderRecognitionSettings;
use crate::ocr::openai_request;

const MAX_BATCH_BYTES: usize = 500 * 1024 * 1024;
const MAX_LINE_BYTES: usize = 6 * 1024 * 1024;

pub struct BatchJsonl {
    pub bytes: Vec<u8>,
    pub rejected: Vec<(String, String)>,
}

#[derive(Clone)]
pub struct AliyunBatchClient {
    client: reqwest::Client,
}

#[derive(Debug, Deserialize)]
struct FileResponse {
    id: String,
}

#[derive(Debug, Deserialize)]
pub struct RemoteBatch {
    pub id: String,
    pub status: String,
    pub input_file_id: Option<String>,
    pub output_file_id: Option<String>,
    pub error_file_id: Option<String>,
    pub errors: Option<Value>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct BatchListResponse {
    data: Vec<RemoteBatch>,
}

impl AliyunBatchClient {
    pub fn new() -> AppResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .map_err(|error| AppError::Internal(error.to_string()))?;
        Ok(Self { client })
    }

    pub async fn check(&self, settings: &OrderRecognitionSettings) -> AppResult<()> {
        if settings.api_key.is_empty() {
            return Err(AppError::InvalidInput("阿里云百炼 API Key 未配置".into()));
        }
        let response = self
            .client
            .get(format!(
                "{}/models",
                settings.base_url.trim_end_matches('/')
            ))
            .bearer_auth(&settings.api_key)
            .send()
            .await
            .map_err(batch_error)?;
        if !response.status().is_success() {
            return Err(response_error(response).await);
        }
        Ok(())
    }

    pub async fn build_jsonl(
        &self,
        app_data_dir: &Path,
        tasks: &[order_import_pool::Model],
        settings: &OrderRecognitionSettings,
    ) -> AppResult<BatchJsonl> {
        let mut output = Vec::new();
        let mut rejected = Vec::new();
        for task in tasks {
            let path =
                match crate::files::resolve_managed_path(app_data_dir, &task.managed_image_path) {
                    Ok(path) => path,
                    Err(error) => {
                        rejected.push((task.id.clone(), error.to_string()));
                        continue;
                    }
                };
            let bytes = match tokio::fs::read(path).await {
                Ok(bytes) => bytes,
                Err(error) => {
                    rejected.push((task.id.clone(), error.to_string()));
                    continue;
                }
            };
            let mime = infer::get(&bytes)
                .map(|kind| kind.mime_type())
                .unwrap_or("image/png");
            let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
            let line = json!({
                "custom_id": task.id,
                "method": "POST",
                "url": "/v1/chat/completions",
                "body": openai_request(
                    &settings.model,
                    &format!("data:{mime};base64,{encoded}"),
                    settings.enable_thinking,
                )
            });
            let mut line =
                serde_json::to_vec(&line).map_err(|error| AppError::Internal(error.to_string()))?;
            if line.len() > MAX_LINE_BYTES {
                rejected.push((
                    task.id.clone(),
                    format!("图片 {} 生成的批量请求超过 6 MB", task.original_file_name),
                ));
                continue;
            }
            line.push(b'\n');
            if output.len() + line.len() > MAX_BATCH_BYTES {
                rejected.push((task.id.clone(), "百炼批量请求文件超过 500 MB".into()));
                continue;
            }
            output.extend(line);
        }
        Ok(BatchJsonl {
            bytes: output,
            rejected,
        })
    }

    pub async fn upload(
        &self,
        settings: &OrderRecognitionSettings,
        jsonl: Vec<u8>,
    ) -> AppResult<String> {
        let part = Part::bytes(jsonl)
            .file_name("order-recognition.jsonl")
            .mime_str("application/jsonl")
            .map_err(|error| AppError::Internal(error.to_string()))?;
        let response = self
            .client
            .post(format!("{}/files", settings.base_url.trim_end_matches('/')))
            .bearer_auth(&settings.api_key)
            .multipart(Form::new().part("file", part).text("purpose", "batch"))
            .send()
            .await
            .map_err(batch_error)?;
        parse_json::<FileResponse>(response)
            .await
            .map(|value| value.id)
    }

    pub async fn create(
        &self,
        settings: &OrderRecognitionSettings,
        input_file_id: &str,
        recovery_name: &str,
    ) -> AppResult<RemoteBatch> {
        let response = self
            .client
            .post(format!(
                "{}/batches",
                settings.base_url.trim_end_matches('/')
            ))
            .bearer_auth(&settings.api_key)
            .json(&json!({
                "input_file_id": input_file_id,
                "endpoint": "/v1/chat/completions",
                "completion_window": "24h",
                "metadata": {
                    "ds_name": recovery_name,
                    "ds_description": "Bill order recognition batch"
                }
            }))
            .send()
            .await
            .map_err(batch_error)?;
        parse_json(response).await
    }

    pub async fn find_by_input_file(
        &self,
        base_url: &str,
        api_key: &str,
        input_file_id: &str,
        recovery_name: &str,
    ) -> AppResult<Option<RemoteBatch>> {
        let response = self
            .client
            .get(format!("{}/batches", base_url.trim_end_matches('/')))
            .bearer_auth(api_key)
            .query(&[
                ("limit", "100"),
                ("input_file_ids", input_file_id),
                ("ds_name", recovery_name),
            ])
            .send()
            .await
            .map_err(batch_error)?;
        let batches = parse_json::<BatchListResponse>(response).await?;
        let mut matching_input = batches
            .data
            .into_iter()
            .filter(|batch| batch.input_file_id.as_deref() == Some(input_file_id))
            .collect::<Vec<_>>();
        if let Some(index) = matching_input.iter().position(|batch| {
            batch
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("ds_name"))
                .and_then(Value::as_str)
                .is_some_and(|name| name == recovery_name)
        }) {
            return Ok(Some(matching_input.swap_remove(index)));
        }
        Ok((matching_input.len() == 1)
            .then(|| matching_input.pop())
            .flatten())
    }

    pub async fn retrieve(
        &self,
        base_url: &str,
        api_key: &str,
        id: &str,
    ) -> AppResult<RemoteBatch> {
        let response = self
            .client
            .get(format!("{}/batches/{id}", base_url.trim_end_matches('/')))
            .bearer_auth(api_key)
            .send()
            .await
            .map_err(batch_error)?;
        parse_json(response).await
    }

    pub async fn cancel(&self, base_url: &str, api_key: &str, id: &str) -> AppResult<RemoteBatch> {
        let response = self
            .client
            .post(format!(
                "{}/batches/{id}/cancel",
                base_url.trim_end_matches('/')
            ))
            .bearer_auth(api_key)
            .send()
            .await
            .map_err(batch_error)?;
        parse_json(response).await
    }

    pub async fn file_content(&self, base_url: &str, api_key: &str, id: &str) -> AppResult<String> {
        let response = self
            .client
            .get(format!(
                "{}/files/{id}/content",
                base_url.trim_end_matches('/')
            ))
            .bearer_auth(api_key)
            .send()
            .await
            .map_err(batch_error)?;
        if !response.status().is_success() {
            return Err(response_error(response).await);
        }
        response.text().await.map_err(batch_error)
    }
}

async fn parse_json<T: for<'de> Deserialize<'de>>(response: reqwest::Response) -> AppResult<T> {
    if !response.status().is_success() {
        return Err(response_error(response).await);
    }
    response.json().await.map_err(batch_error)
}

async fn response_error(response: reqwest::Response) -> AppError {
    let status = response.status();
    let detail = response.text().await.unwrap_or_default();
    AppError::AliyunBatch(format!(
        "HTTP {status}：{}",
        detail.chars().take(500).collect::<String>()
    ))
}

fn batch_error(error: impl std::fmt::Display) -> AppError {
    AppError::AliyunBatch(error.to_string())
}

pub fn terminal_status(status: &str) -> bool {
    matches!(
        status,
        "completed" | "failed" | "expired" | "cancelled" | "canceled"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::OrderRecognitionProvider;

    fn task(id: &str, path: &str) -> order_import_pool::Model {
        order_import_pool::Model {
            id: id.into(),
            original_file_name: format!("{id}.png"),
            managed_image_path: path.into(),
            image_sha256: format!("hash-{id}"),
            status: "pending".into(),
            recognition_batch_id: None,
            error_code: None,
            error_message: None,
            retry_count: 0,
            created_at: "2026-08-13T00:00:00Z".into(),
            started_at: None,
        }
    }

    #[tokio::test]
    async fn builds_aliyun_vision_jsonl_and_isolates_unreadable_tasks() {
        let app_data = tempfile::tempdir().unwrap();
        std::fs::create_dir(app_data.path().join("images")).unwrap();
        std::fs::write(app_data.path().join("images/valid.png"), b"image bytes").unwrap();
        let tasks = vec![
            task("valid", "images/valid.png"),
            task("missing", "images/missing.png"),
        ];
        let settings = OrderRecognitionSettings {
            provider: OrderRecognitionProvider::AliyunBatch,
            base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1".into(),
            model: "qwen3-vl-flash".into(),
            api_key: "secret".into(),
            max_concurrency: 3,
            enable_thinking: true,
        };

        let jsonl = AliyunBatchClient::new()
            .unwrap()
            .build_jsonl(app_data.path(), &tasks, &settings)
            .await
            .unwrap();

        assert_eq!(jsonl.rejected.len(), 1);
        assert_eq!(jsonl.rejected[0].0, "missing");
        let line: Value = serde_json::from_slice(jsonl.bytes.strip_suffix(b"\n").unwrap()).unwrap();
        assert_eq!(line["custom_id"], "valid");
        assert_eq!(line["method"], "POST");
        assert_eq!(line["url"], "/v1/chat/completions");
        assert_eq!(line["body"]["model"], "qwen3-vl-flash");
        assert_eq!(line["body"]["enable_thinking"], true);
        assert_eq!(line["body"]["response_format"]["type"], "json_schema");
        assert!(
            line["body"]["messages"][0]["content"][1]["image_url"]["url"]
                .as_str()
                .unwrap()
                .starts_with("data:image/png;base64,")
        );
    }

    #[test]
    fn recognizes_all_remote_terminal_states() {
        for status in ["completed", "failed", "expired", "cancelled", "canceled"] {
            assert!(terminal_status(status));
        }
        assert!(!terminal_status("cancelling"));
        assert!(!terminal_status("in_progress"));
    }
}
