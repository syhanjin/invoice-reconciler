use serde::{Deserialize, Serialize};

pub const DEFAULT_MAX_CONCURRENCY: u8 = 3;
pub const MIN_CONCURRENCY: u8 = 1;
pub const MAX_CONCURRENCY: u8 = 8;

#[derive(Clone, Debug, Deserialize, Serialize, sea_orm::FromQueryResult)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub ollama_base_url: String,
    pub ollama_model: String,
    pub max_concurrency: u8,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsRequest {
    pub ollama_base_url: String,
    pub ollama_model: String,
    pub max_concurrency: u8,
}

impl UpdateSettingsRequest {
    pub fn validate(self) -> crate::error::AppResult<AppSettings> {
        let base_url = self.ollama_base_url.trim().trim_end_matches('/');
        let model = self.ollama_model.trim();
        if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
            return Err(crate::error::AppError::InvalidInput(
                "Ollama 地址必须以 http:// 或 https:// 开头".into(),
            ));
        }
        if model.is_empty() {
            return Err(crate::error::AppError::InvalidInput(
                "模型名称不能为空".into(),
            ));
        }
        if !(MIN_CONCURRENCY..=MAX_CONCURRENCY).contains(&self.max_concurrency) {
            return Err(crate::error::AppError::InvalidInput(
                "最大并发数必须在 1 到 8 之间".into(),
            ));
        }
        Ok(AppSettings {
            ollama_base_url: base_url.to_owned(),
            ollama_model: model.to_owned(),
            max_concurrency: self.max_concurrency,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateBatchRequest {
    pub platform: Platform,
    pub image_paths: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Taobao,
}

impl Platform {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Taobao => "taobao",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct OcrOrder {
    pub order_number: Option<String>,
    pub product_names: Vec<String>,
    pub paid_amount: Option<serde_json::Number>,
    pub ordered_at: Option<String>,
    pub shop_name: Option<String>,
}

#[derive(Clone, Debug)]
pub struct NormalizedOrder {
    pub order_number: Option<String>,
    pub product_names: Vec<String>,
    pub paid_amount_fen: Option<i64>,
    pub ordered_at: Option<String>,
    pub shop_name: Option<String>,
    pub warning: Option<String>,
    pub raw_json: String,
}

#[derive(Clone, Debug, Serialize, sea_orm::FromQueryResult)]
#[serde(rename_all = "camelCase")]
pub struct ImportTask {
    pub id: String,
    pub batch_id: String,
    pub original_file_name: String,
    pub managed_image_path: Option<String>,
    pub image_sha256: String,
    pub status: String,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub warning_message: Option<String>,
    pub retry_count: i64,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchCounts {
    pub total: i64,
    pub queued: i64,
    pub processing: i64,
    pub succeeded: i64,
    pub failed: i64,
    pub canceled: i64,
    pub skipped: i64,
    pub interrupted: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchDetail {
    pub id: String,
    pub platform: String,
    pub status: String,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub counts: BatchCounts,
    pub tasks: Vec<ImportTask>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskUpdateEvent {
    pub task: ImportTask,
    pub batch: BatchDetail,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapState {
    pub settings: AppSettings,
    pub latest_batch: Option<BatchDetail>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OllamaStatus {
    pub reachable: bool,
    pub model_available: bool,
    pub version: Option<String>,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrdersQuery {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
    pub search: Option<String>,
    pub duplicate_only: Option<bool>,
    pub sort_by: Option<OrderSort>,
    pub sort_direction: Option<SortDirection>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderSort {
    CreatedAt,
    OrderedAt,
    PaidAmount,
    ShopName,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortDirection {
    Asc,
    Desc,
}

#[derive(Clone, Debug, Serialize, sea_orm::FromQueryResult)]
#[serde(rename_all = "camelCase")]
pub struct OrderListItem {
    pub id: String,
    pub platform: String,
    pub external_order_number: Option<String>,
    pub paid_amount_fen: Option<i64>,
    pub ordered_at: Option<String>,
    pub shop_name: Option<String>,
    pub possible_duplicate: bool,
    pub created_at: String,
    pub product_summary: String,
    pub product_count: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrdersPage {
    pub items: Vec<OrderListItem>,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderDetail {
    pub id: String,
    pub platform: String,
    pub external_order_number: Option<String>,
    pub product_names: Vec<String>,
    pub paid_amount_fen: Option<i64>,
    pub ordered_at: Option<String>,
    pub shop_name: Option<String>,
    pub raw_ocr_json: String,
    pub possible_duplicate: bool,
    pub created_at: String,
    pub updated_at: String,
    pub original_file_name: String,
    pub image_path: String,
    pub warning_message: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateOrderRequest {
    pub id: String,
    pub external_order_number: Option<String>,
    pub product_names: Vec<String>,
    pub paid_amount_fen: Option<i64>,
    pub ordered_at: Option<String>,
    pub shop_name: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdRequest {
    pub id: String,
}
