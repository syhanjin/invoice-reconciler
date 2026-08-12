use std::path::Path;
use std::time::Duration;

use base64::Engine;
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::json;

use crate::error::{AppError, AppResult};
use crate::models::{
    OcrOrder, OrderRecognitionProvider, OrderRecognitionSettings, RecognizerStatus,
};
use crate::normalize::normalize_ocr;

const TAOBAO_PROMPT: &str = r#"这是一张淘宝订单截图，一张截图只允许表示一个订单，一个订单可以包含多个商品。
请仔细识别订单号、所有商品名称、实付金额、下单/创建时间和店铺名称。
不存在的单值字段返回 null；没有商品时返回空数组。只输出符合所给 JSON Schema 的对象。"#;

#[derive(Clone)]
pub struct OllamaClient {
    check_client: reqwest::Client,
    ocr_client: reqwest::Client,
}

#[derive(Deserialize)]
struct VersionResponse {
    version: Option<String>,
}

#[derive(Deserialize)]
struct TagsResponse {
    models: Vec<TagModel>,
}

#[derive(Deserialize)]
struct TagModel {
    name: String,
    model: Option<String>,
}

#[derive(Deserialize)]
struct ChatResponse {
    message: ChatMessage,
}

#[derive(Deserialize)]
struct ChatMessage {
    content: String,
}

impl OllamaClient {
    pub fn new() -> AppResult<Self> {
        let check_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(8))
            .build()
            .map_err(|error| AppError::Internal(error.to_string()))?;
        let ocr_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .map_err(|error| AppError::Internal(error.to_string()))?;
        Ok(Self {
            check_client,
            ocr_client,
        })
    }

    pub async fn check(&self, settings: &OrderRecognitionSettings) -> AppResult<RecognizerStatus> {
        if !matches!(settings.provider, OrderRecognitionProvider::Ollama) {
            return self.check_openai(settings).await;
        }
        let base_url = settings.base_url.trim_end_matches('/');
        let version = self
            .check_client
            .get(format!("{base_url}/api/version"))
            .send()
            .await
            .map_err(|error| AppError::OllamaUnavailable(error.to_string()))?
            .error_for_status()
            .map_err(|error| AppError::OllamaUnavailable(error.to_string()))?
            .json::<VersionResponse>()
            .await
            .map_err(|error| AppError::OllamaUnavailable(error.to_string()))?;
        let tags = self
            .check_client
            .get(format!("{base_url}/api/tags"))
            .send()
            .await
            .map_err(|error| AppError::OllamaUnavailable(error.to_string()))?
            .error_for_status()
            .map_err(|error| AppError::OllamaUnavailable(error.to_string()))?
            .json::<TagsResponse>()
            .await
            .map_err(|error| AppError::OllamaUnavailable(error.to_string()))?;
        let model_available = tags.models.iter().any(|entry| {
            entry.name == settings.model || entry.model.as_deref() == Some(settings.model.as_str())
        });
        Ok(RecognizerStatus {
            provider: settings.provider,
            reachable: true,
            model_available,
            version: version.version,
            message: if model_available {
                "Ollama 与视觉模型可用".into()
            } else {
                format!(
                    "未找到模型 {}，请运行：ollama pull {}",
                    settings.model, settings.model
                )
            },
        })
    }

    pub async fn recognize(
        &self,
        settings: &OrderRecognitionSettings,
        image_path: &Path,
    ) -> AppResult<crate::models::NormalizedOrder> {
        let bytes = tokio::fs::read(image_path).await?;
        let image = base64::engine::general_purpose::STANDARD.encode(bytes);
        if matches!(settings.provider, OrderRecognitionProvider::Openai) {
            return self.recognize_openai(settings, image_path).await;
        }
        let request = json!({
            "model": settings.model,
            "messages": [{
                "role": "user",
                "content": TAOBAO_PROMPT,
                "images": [image]
            }],
            "stream": false,
            "format": order_schema(),
            "options": {
                "num_ctx": 16384,
                "temperature": 0
            }
        });
        let response = self
            .ocr_client
            .post(format!(
                "{}/api/chat",
                settings.base_url.trim_end_matches('/')
            ))
            .json(&request)
            .send()
            .await
            .map_err(|error| AppError::OllamaUnavailable(error.to_string()))?;
        if response.status() == StatusCode::NOT_FOUND {
            return Err(AppError::ModelMissing(settings.model.clone()));
        }
        if response.status() == StatusCode::SERVICE_UNAVAILABLE {
            return Err(AppError::OllamaUnavailable(
                "Ollama 队列已满（503），请稍后重试或降低并发数".into(),
            ));
        }
        let response = response
            .error_for_status()
            .map_err(|error| AppError::OllamaUnavailable(error.to_string()))?
            .json::<ChatResponse>()
            .await
            .map_err(|error| AppError::InvalidOcr(format!("无法解析 Ollama 响应：{error}")))?;
        let raw_json = response.message.content.trim().to_owned();
        let order: OcrOrder = serde_json::from_str(&raw_json)
            .map_err(|error| AppError::InvalidOcr(format!("JSON Schema 校验失败：{error}")))?;
        normalize_ocr(order, raw_json)
    }

    async fn check_openai(
        &self,
        settings: &OrderRecognitionSettings,
    ) -> AppResult<RecognizerStatus> {
        let mut request = self.check_client.get(format!(
            "{}/models",
            settings.base_url.trim_end_matches('/')
        ));
        if !settings.api_key.is_empty() {
            request = request.bearer_auth(&settings.api_key);
        }
        let response = request
            .send()
            .await
            .map_err(|error| AppError::RecognitionService(error.to_string()))?;
        if !response.status().is_success() {
            return Err(AppError::RecognitionService(format!(
                "服务返回 HTTP {}",
                response.status()
            )));
        }
        let payload: serde_json::Value = response
            .json()
            .await
            .map_err(|error| AppError::RecognitionService(format!("无法解析模型列表：{error}")))?;
        let model_available = payload["data"]
            .as_array()
            .is_none_or(|models| models.iter().any(|entry| entry["id"] == settings.model));
        Ok(RecognizerStatus {
            provider: settings.provider,
            reachable: true,
            model_available,
            version: None,
            message: if model_available {
                "识别服务与模型可用".into()
            } else {
                format!("服务可连接，但模型列表中未找到 {}", settings.model)
            },
        })
    }

    async fn recognize_openai(
        &self,
        settings: &OrderRecognitionSettings,
        image_path: &Path,
    ) -> AppResult<crate::models::NormalizedOrder> {
        let bytes = tokio::fs::read(image_path).await?;
        let mime = infer::get(&bytes)
            .map(|kind| kind.mime_type())
            .unwrap_or("image/png");
        let image = base64::engine::general_purpose::STANDARD.encode(bytes);
        let request = openai_request(
            &settings.model,
            &format!("data:{mime};base64,{image}"),
            false,
        );
        let mut call = self
            .ocr_client
            .post(format!(
                "{}/chat/completions",
                settings.base_url.trim_end_matches('/')
            ))
            .json(&request);
        if !settings.api_key.is_empty() {
            call = call.bearer_auth(&settings.api_key);
        }
        let response = call
            .send()
            .await
            .map_err(|error| AppError::RecognitionService(error.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            let detail = response.text().await.unwrap_or_default();
            return Err(AppError::RecognitionService(format!(
                "HTTP {status}：{}",
                detail.chars().take(300).collect::<String>()
            )));
        }
        let response = response
            .json::<OpenAiChatResponse>()
            .await
            .map_err(|error| AppError::InvalidOcr(format!("无法解析识别响应：{error}")))?;
        let content = response
            .choices
            .into_iter()
            .next()
            .and_then(|choice| choice.message.content)
            .ok_or_else(|| AppError::InvalidOcr("识别响应没有返回内容".into()))?;
        normalize_content(&content)
    }
}

#[derive(Deserialize)]
struct OpenAiChatResponse {
    choices: Vec<OpenAiChoice>,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
}

#[derive(Deserialize)]
struct OpenAiMessage {
    content: Option<String>,
}

pub(crate) fn openai_request(
    model: &str,
    image_url: &str,
    enable_thinking: bool,
) -> serde_json::Value {
    let mut request = json!({
        "model": model,
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": TAOBAO_PROMPT},
                {"type": "image_url", "image_url": {"url": image_url}}
            ]
        }],
        "temperature": 0,
        "response_format": {
            "type": "json_schema",
            "json_schema": {"name": "taobao_order", "strict": true, "schema": order_schema()}
        }
    });
    if enable_thinking {
        request["enable_thinking"] = json!(true);
    }
    request
}

pub(crate) fn normalize_content(content: &str) -> AppResult<crate::models::NormalizedOrder> {
    let raw_json = content
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
        .to_owned();
    let order: OcrOrder = serde_json::from_str(&raw_json)
        .map_err(|error| AppError::InvalidOcr(format!("JSON Schema 校验失败：{error}")))?;
    normalize_ocr(order, raw_json)
}

pub(crate) fn order_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "order_number": {"type": ["string", "null"]},
            "product_names": {"type": "array", "items": {"type": "string"}},
            "paid_amount": {"type": ["number", "null"], "minimum": 0},
            "ordered_at": {"type": ["string", "null"]},
            "shop_name": {"type": ["string", "null"]}
        },
        "required": ["order_number", "product_names", "paid_amount", "ordered_at", "shop_name"]
    })
}

#[cfg(test)]
mod tests {
    use std::future::IntoFuture;
    use std::sync::Arc;

    use axum::{Json, Router, http::HeaderMap, routing::post};
    use serde_json::{Value, json};
    use tempfile::NamedTempFile;
    use tokio::sync::Mutex;

    use super::*;

    #[test]
    fn schema_requires_every_normalized_field() {
        let schema = order_schema();
        assert_eq!(schema["required"].as_array().unwrap().len(), 5);
        assert_eq!(schema["additionalProperties"], false);
    }

    #[tokio::test]
    async fn sends_structured_vision_request_and_normalizes_response() {
        let captured = Arc::new(Mutex::new(None::<Value>));
        let route_capture = captured.clone();
        let router = Router::new().route(
            "/api/chat",
            post(move |Json(body): Json<Value>| {
                let route_capture = route_capture.clone();
                async move {
                    *route_capture.lock().await = Some(body);
                    Json(json!({
                        "message": {
                            "content": "{\"order_number\":\"TB-1\",\"product_names\":[\"商品 A\"],\"paid_amount\":19.99,\"ordered_at\":\"2026年08月12日 09:30\",\"shop_name\":\"示例店铺\"}"
                        }
                    }))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(axum::serve(listener, router).into_future());

        let image = NamedTempFile::new().unwrap();
        std::fs::write(image.path(), b"image bytes").unwrap();
        let settings = OrderRecognitionSettings {
            provider: OrderRecognitionProvider::Ollama,
            base_url: format!("http://{address}"),
            model: "vision:test".into(),
            api_key: String::new(),
            max_concurrency: 3,
            enable_thinking: false,
        };
        let result = OllamaClient::new()
            .unwrap()
            .recognize(&settings, image.path())
            .await
            .unwrap();

        assert_eq!(result.order_number.as_deref(), Some("TB-1"));
        assert_eq!(result.product_names, vec!["商品 A"]);
        assert_eq!(result.paid_amount_fen, Some(1999));
        assert_eq!(result.ordered_at.as_deref(), Some("2026-08-12 09:30"));
        let body = captured.lock().await.clone().unwrap();
        assert_eq!(body["model"], "vision:test");
        assert_eq!(body["stream"], false);
        assert_eq!(body["options"]["temperature"], 0);
        assert_eq!(body["format"]["additionalProperties"], false);
        assert!(
            !body["messages"][0]["images"][0]
                .as_str()
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn sends_openai_compatible_request_with_bearer_auth() {
        let captured = Arc::new(Mutex::new(None::<(HeaderMap, Value)>));
        let route_capture = captured.clone();
        let router = Router::new().route(
            "/v1/chat/completions",
            post(move |headers: HeaderMap, Json(body): Json<Value>| {
                let route_capture = route_capture.clone();
                async move {
                    *route_capture.lock().await = Some((headers, body));
                    Json(json!({
                        "choices": [{
                            "message": {
                                "content": "```json\n{\"order_number\":\"OPENAI-1\",\"product_names\":[\"商品 B\"],\"paid_amount\":8.88,\"ordered_at\":null,\"shop_name\":\"兼容服务\"}\n```"
                            }
                        }]
                    }))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(axum::serve(listener, router).into_future());

        let image = NamedTempFile::new().unwrap();
        std::fs::write(image.path(), b"image bytes").unwrap();
        let settings = OrderRecognitionSettings {
            provider: OrderRecognitionProvider::Openai,
            base_url: format!("http://{address}/v1"),
            model: "compatible-vision".into(),
            api_key: "secret-key".into(),
            max_concurrency: 2,
            enable_thinking: false,
        };
        let result = OllamaClient::new()
            .unwrap()
            .recognize(&settings, image.path())
            .await
            .unwrap();

        assert_eq!(result.order_number.as_deref(), Some("OPENAI-1"));
        assert_eq!(result.paid_amount_fen, Some(888));
        let (headers, body) = captured.lock().await.clone().unwrap();
        assert_eq!(headers["authorization"], "Bearer secret-key");
        assert_eq!(body["model"], "compatible-vision");
        assert_eq!(body["response_format"]["type"], "json_schema");
        assert_eq!(body["response_format"]["json_schema"]["strict"], true);
        assert!(
            body["messages"][0]["content"][1]["image_url"]["url"]
                .as_str()
                .unwrap()
                .starts_with("data:image/png;base64,")
        );
    }
}
