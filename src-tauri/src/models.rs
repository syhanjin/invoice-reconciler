use serde::{Deserialize, Serialize};

pub const DEFAULT_MAX_CONCURRENCY: u8 = 3;
pub const MIN_CONCURRENCY: u8 = 1;
pub const MAX_CONCURRENCY: u8 = 8;
pub const DEFAULT_INVOICE_OCR_CONCURRENCY: u8 = 3;
pub const MAX_INVOICE_OCR_CONCURRENCY: u8 = 5;
pub const DEFAULT_MATCH_MAX_ORDERS: u8 = 10;
pub const DEFAULT_MATCH_MAX_CANDIDATES: u16 = 100;
pub const MIN_MATCH_MAX_ORDERS: u8 = 2;
pub const MAX_MATCH_MAX_ORDERS: u8 = 20;
pub const MIN_MATCH_MAX_CANDIDATES: u16 = 10;
pub const MAX_MATCH_MAX_CANDIDATES: u16 = 500;
pub const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
pub const DEFAULT_OPENAI_MODEL: &str = "gpt-4o-mini";

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OrderRecognitionProvider {
    #[default]
    Ollama,
    Openai,
}

impl OrderRecognitionProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ollama => "ollama",
            Self::Openai => "openai",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub order_recognition_provider: OrderRecognitionProvider,
    pub ollama_base_url: String,
    pub ollama_model: String,
    pub openai_base_url: String,
    pub openai_model: String,
    pub openai_api_key_masked: Option<String>,
    pub openai_api_key_configured: bool,
    pub max_concurrency: u8,
    pub tencent_secret_id_masked: Option<String>,
    pub tencent_credentials_configured: bool,
    pub invoice_ocr_concurrency: u8,
    pub match_max_orders: u8,
    pub match_max_candidates: u16,
    pub settings_path: String,
    pub permission_warning: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsRequest {
    pub order_recognition_provider: OrderRecognitionProvider,
    pub ollama_base_url: String,
    pub ollama_model: String,
    pub openai_base_url: String,
    pub openai_model: String,
    pub openai_api_key: Option<String>,
    pub clear_openai_api_key: bool,
    pub max_concurrency: u8,
    pub tencent_secret_id: Option<String>,
    pub tencent_secret_key: Option<String>,
    pub clear_tencent_credentials: bool,
    pub invoice_ocr_concurrency: u8,
    pub match_max_orders: u8,
    pub match_max_candidates: u16,
}

impl UpdateSettingsRequest {
    pub fn validate(self) -> crate::error::AppResult<AppSettings> {
        let ollama_base_url = validate_url("Ollama 地址", &self.ollama_base_url)?;
        let openai_base_url = validate_url("OpenAI 兼容地址", &self.openai_base_url)?;
        let ollama_model = validate_model("Ollama 模型", &self.ollama_model)?;
        let openai_model = validate_model("OpenAI 兼容模型", &self.openai_model)?;
        if !(MIN_CONCURRENCY..=MAX_CONCURRENCY).contains(&self.max_concurrency) {
            return Err(crate::error::AppError::InvalidInput(
                "最大并发数必须在 1 到 8 之间".into(),
            ));
        }
        if !(MIN_CONCURRENCY..=MAX_INVOICE_OCR_CONCURRENCY).contains(&self.invoice_ocr_concurrency)
        {
            return Err(crate::error::AppError::InvalidInput(
                "腾讯云 OCR 并发数必须在 1 到 5 之间".into(),
            ));
        }
        if !(MIN_MATCH_MAX_ORDERS..=MAX_MATCH_MAX_ORDERS).contains(&self.match_max_orders) {
            return Err(crate::error::AppError::InvalidInput(
                "自动组合最大订单数必须在 2 到 20 之间".into(),
            ));
        }
        if !(MIN_MATCH_MAX_CANDIDATES..=MAX_MATCH_MAX_CANDIDATES)
            .contains(&self.match_max_candidates)
        {
            return Err(crate::error::AppError::InvalidInput(
                "自动候选组合数必须在 10 到 500 之间".into(),
            ));
        }
        Ok(AppSettings {
            order_recognition_provider: self.order_recognition_provider,
            ollama_base_url,
            ollama_model,
            openai_base_url,
            openai_model,
            openai_api_key_masked: None,
            openai_api_key_configured: false,
            max_concurrency: self.max_concurrency,
            tencent_secret_id_masked: None,
            tencent_credentials_configured: false,
            invoice_ocr_concurrency: self.invoice_ocr_concurrency,
            match_max_orders: self.match_max_orders,
            match_max_candidates: self.match_max_candidates,
            settings_path: String::new(),
            permission_warning: None,
        })
    }
}

#[derive(Clone, Debug)]
pub struct PrivateAppSettings {
    pub order_recognition_provider: OrderRecognitionProvider,
    pub ollama_base_url: String,
    pub ollama_model: String,
    pub openai_base_url: String,
    pub openai_model: String,
    pub openai_api_key: String,
    pub max_concurrency: u8,
    pub tencent_secret_id: String,
    pub tencent_secret_key: String,
    pub invoice_ocr_concurrency: u8,
    pub match_max_orders: u8,
    pub match_max_candidates: u16,
    pub settings_path: String,
    pub permission_warning: Option<String>,
}

impl PrivateAppSettings {
    pub fn public(&self) -> AppSettings {
        AppSettings {
            order_recognition_provider: self.order_recognition_provider,
            ollama_base_url: self.ollama_base_url.clone(),
            ollama_model: self.ollama_model.clone(),
            openai_base_url: self.openai_base_url.clone(),
            openai_model: self.openai_model.clone(),
            openai_api_key_masked: mask_secret_id(&self.openai_api_key),
            openai_api_key_configured: !self.openai_api_key.is_empty(),
            max_concurrency: self.max_concurrency,
            tencent_secret_id_masked: mask_secret_id(&self.tencent_secret_id),
            tencent_credentials_configured: !self.tencent_secret_id.is_empty()
                && !self.tencent_secret_key.is_empty(),
            invoice_ocr_concurrency: self.invoice_ocr_concurrency,
            match_max_orders: self.match_max_orders,
            match_max_candidates: self.match_max_candidates,
            settings_path: self.settings_path.clone(),
            permission_warning: self.permission_warning.clone(),
        }
    }

    pub fn order_recognition(&self) -> OrderRecognitionSettings {
        OrderRecognitionSettings {
            provider: self.order_recognition_provider,
            base_url: match self.order_recognition_provider {
                OrderRecognitionProvider::Ollama => self.ollama_base_url.clone(),
                OrderRecognitionProvider::Openai => self.openai_base_url.clone(),
            },
            model: match self.order_recognition_provider {
                OrderRecognitionProvider::Ollama => self.ollama_model.clone(),
                OrderRecognitionProvider::Openai => self.openai_model.clone(),
            },
            api_key: match self.order_recognition_provider {
                OrderRecognitionProvider::Ollama => String::new(),
                OrderRecognitionProvider::Openai => self.openai_api_key.clone(),
            },
            max_concurrency: self.max_concurrency,
        }
    }
}

#[derive(Clone, Debug)]
pub struct OrderRecognitionSettings {
    pub provider: OrderRecognitionProvider,
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub max_concurrency: u8,
}

fn validate_url(label: &str, value: &str) -> crate::error::AppResult<String> {
    let value = value.trim().trim_end_matches('/');
    if !(value.starts_with("http://") || value.starts_with("https://")) {
        return Err(crate::error::AppError::InvalidInput(format!(
            "{label}必须以 http:// 或 https:// 开头"
        )));
    }
    Ok(value.to_owned())
}

fn validate_model(label: &str, value: &str) -> crate::error::AppResult<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(crate::error::AppError::InvalidInput(format!(
            "{label}不能为空"
        )));
    }
    Ok(value.to_owned())
}

fn mask_secret_id(value: &str) -> Option<String> {
    if value.is_empty() {
        return None;
    }
    let chars = value.chars().collect::<Vec<_>>();
    if chars.len() <= 8 {
        return Some("********".into());
    }
    Some(format!(
        "{}****{}",
        chars[..4].iter().collect::<String>(),
        chars[chars.len() - 4..].iter().collect::<String>()
    ))
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateBatchRequest {
    pub platform: Platform,
    pub image_paths: Vec<String>,
}

pub type ImportOrderImagesRequest = CreateBatchRequest;

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

#[cfg(test)]
#[derive(Clone, Debug, Serialize)]
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

#[cfg(test)]
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

#[cfg(test)]
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportBatchesQuery {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

#[cfg(test)]
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportBatchSummary {
    pub id: String,
    pub platform: String,
    pub status: String,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub counts: BatchCounts,
}

#[cfg(test)]
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportBatchesPage {
    pub items: Vec<ImportBatchSummary>,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderImportTask {
    pub id: String,
    pub original_file_name: String,
    pub status: String,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub retry_count: i64,
    pub created_at: String,
    pub started_at: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderImportPool {
    pub tasks: Vec<OrderImportTask>,
    pub pending: i64,
    pub processing: i64,
    pub failed: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOrderImagesResult {
    pub imported: usize,
    pub skipped: usize,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderImportLogsQuery {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

#[derive(Clone, Debug, Serialize, sea_orm::FromQueryResult)]
#[serde(rename_all = "camelCase")]
pub struct OrderImportLog {
    pub task_id: String,
    pub order_id: String,
    pub original_file_name: String,
    pub provider: String,
    pub model: String,
    pub warning_message: Option<String>,
    pub completed_at: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderImportLogsPage {
    pub items: Vec<OrderImportLog>,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderImportUpdateEvent {
    pub kind: String,
    pub task_id: Option<String>,
    pub order_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapState {
    pub settings: AppSettings,
    pub latest_invoice_batch: Option<InvoiceBatchDetail>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecognizerStatus {
    pub provider: OrderRecognitionProvider,
    pub reachable: bool,
    pub model_available: bool,
    pub version: Option<String>,
    pub message: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
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

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteOrdersRequest {
    pub ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteOrderResult {
    pub id: String,
    pub deleted: bool,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteOrdersResult {
    pub results: Vec<DeleteOrderResult>,
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
    pub is_matched: bool,
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

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareInvoiceBatchRequest {
    pub file_paths: Vec<String>,
    pub forced_type: Option<i32>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateInvoiceBatchRequest {
    pub preflight_token: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvoicePreflightFile {
    pub path: String,
    pub original_file_name: String,
    pub media_type: String,
    pub size_bytes: u64,
    pub pdf_pages: u32,
    pub status: String,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceBatchPreview {
    pub preflight_token: String,
    pub selected_count: usize,
    pub valid_count: usize,
    pub duplicate_count: usize,
    pub invalid_count: usize,
    pub predicted_requests: usize,
    pub total_pdf_pages: u32,
    pub forced_type: Option<i32>,
    pub files: Vec<InvoicePreflightFile>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceTask {
    pub id: String,
    pub batch_id: String,
    pub original_file_name: String,
    pub source_path: Option<String>,
    pub source_sha256: String,
    pub media_type: String,
    pub forced_type: Option<i32>,
    pub status: String,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub warning_message: Option<String>,
    pub retry_count: i64,
    pub request_id: Option<String>,
    pub existing_invoice_id: Option<String>,
    pub review_items: Vec<InvoiceReviewItem>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceReviewItem {
    pub index: usize,
    pub page: i32,
    pub subtype: String,
    pub subtype_description: String,
    pub invoice_number: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceBatchCounts {
    pub total: i64,
    pub queued: i64,
    pub processing: i64,
    pub awaiting_review: i64,
    pub succeeded: i64,
    pub failed: i64,
    pub canceled: i64,
    pub interrupted: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceBatchDetail {
    pub id: String,
    pub status: String,
    pub forced_type: Option<i32>,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub counts: InvoiceBatchCounts,
    pub tasks: Vec<InvoiceTask>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceBatchSummary {
    pub id: String,
    pub status: String,
    pub forced_type: Option<i32>,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub counts: InvoiceBatchCounts,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceBatchesQuery {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceBatchesPage {
    pub items: Vec<InvoiceBatchSummary>,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceTaskUpdateEvent {
    pub task: InvoiceTask,
    pub batch: InvoiceBatchDetail,
}

#[derive(Clone, Debug)]
pub struct NormalizedInvoiceItem {
    pub name: String,
    pub specification: Option<String>,
    pub unit: Option<String>,
    pub quantity: Option<String>,
    pub unit_price_fen: Option<i64>,
    pub amount_fen: Option<i64>,
    pub tax_rate: Option<String>,
    pub tax_fen: Option<i64>,
    pub raw_json: String,
}

#[derive(Clone, Debug)]
pub struct NormalizedInvoice {
    pub id: String,
    pub type_id: i32,
    pub subtype: String,
    pub type_description: String,
    pub subtype_description: String,
    pub invoice_code: Option<String>,
    pub invoice_number: Option<String>,
    pub invoice_date: Option<String>,
    pub buyer_name: Option<String>,
    pub buyer_tax_id: Option<String>,
    pub seller_name: Option<String>,
    pub seller_tax_id: Option<String>,
    pub amount_fen: Option<i64>,
    pub tax_fen: Option<i64>,
    pub total_fen: Option<i64>,
    pub currency: String,
    pub reliable_identity: Option<String>,
    pub possible_duplicate: bool,
    pub preview_path: String,
    pub page_start: i32,
    pub page_end: i32,
    pub raw_invoice_json: String,
    pub items: Vec<NormalizedInvoiceItem>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceSort {
    InvoiceDate,
    TotalAmount,
    CreatedAt,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvoicesQuery {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
    pub search: Option<String>,
    pub subtype: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub duplicate_only: Option<bool>,
    pub sort_by: Option<InvoiceSort>,
    pub sort_direction: Option<SortDirection>,
}

#[derive(Clone, Debug, Serialize, sea_orm::FromQueryResult)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceListItem {
    pub id: String,
    pub type_id: i32,
    pub subtype: String,
    pub subtype_description: String,
    pub invoice_code: Option<String>,
    pub invoice_number: Option<String>,
    pub invoice_date: Option<String>,
    pub buyer_name: Option<String>,
    pub seller_name: Option<String>,
    pub total_fen: Option<i64>,
    pub currency: String,
    pub possible_duplicate: bool,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvoicesPage {
    pub items: Vec<InvoiceListItem>,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceLineItem {
    pub id: i32,
    pub position: i32,
    pub name: String,
    pub specification: Option<String>,
    pub unit: Option<String>,
    pub quantity: Option<String>,
    pub unit_price_fen: Option<i64>,
    pub amount_fen: Option<i64>,
    pub tax_rate: Option<String>,
    pub tax_fen: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceDetail {
    pub id: String,
    pub task_id: String,
    pub type_id: i32,
    pub subtype: String,
    pub type_description: String,
    pub subtype_description: String,
    pub invoice_code: Option<String>,
    pub invoice_number: Option<String>,
    pub invoice_date: Option<String>,
    pub buyer_name: Option<String>,
    pub buyer_tax_id: Option<String>,
    pub seller_name: Option<String>,
    pub seller_tax_id: Option<String>,
    pub amount_fen: Option<i64>,
    pub tax_fen: Option<i64>,
    pub total_fen: Option<i64>,
    pub currency: String,
    pub possible_duplicate: bool,
    pub preview_path: String,
    pub source_path: String,
    pub original_file_name: String,
    pub page_start: i32,
    pub page_end: i32,
    pub raw_invoice_json: String,
    pub raw_response_json: String,
    pub request_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub items: Vec<InvoiceLineItem>,
    pub is_matched: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInvoiceRequest {
    pub id: String,
    pub invoice_code: Option<String>,
    pub invoice_number: Option<String>,
    pub invoice_date: Option<String>,
    pub buyer_name: Option<String>,
    pub buyer_tax_id: Option<String>,
    pub seller_name: Option<String>,
    pub seller_tax_id: Option<String>,
    pub amount_fen: Option<i64>,
    pub tax_fen: Option<i64>,
    pub total_fen: Option<i64>,
    pub currency: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteInvoicesRequest {
    pub ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteInvoiceResult {
    pub id: String,
    pub deleted: bool,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteInvoicesResult {
    pub results: Vec<DeleteInvoiceResult>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryInvoiceTaskRequest {
    pub id: String,
    pub forced_type: Option<i32>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmInvoiceSplitRequest {
    pub task_id: String,
    pub groups: Vec<Vec<usize>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchingQuery {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
    pub search: Option<String>,
    pub status: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchingSummary {
    pub total: i64,
    pub suggested: i64,
    pub conflict: i64,
    pub unmatched: i64,
    pub ineligible: i64,
    pub confirmed: i64,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MatchOrderSummary {
    pub id: String,
    pub external_order_number: Option<String>,
    pub paid_amount_fen: i64,
    pub ordered_at: Option<String>,
    pub shop_name: Option<String>,
    pub product_summary: String,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MatchCandidate {
    pub order_ids: Vec<String>,
    pub orders: Vec<MatchOrderSummary>,
    pub total_fen: i64,
    pub shop_name: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchingInvoiceItem {
    pub id: String,
    pub invoice_number: Option<String>,
    pub subtype_description: String,
    pub invoice_date: Option<String>,
    pub seller_name: Option<String>,
    pub total_fen: Option<i64>,
    pub currency: String,
    pub status: String,
    pub reason: Option<String>,
    pub candidates: Vec<MatchCandidate>,
    pub confirmed_source: Option<String>,
    pub confirmed_at: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchingPage {
    pub items: Vec<MatchingInvoiceItem>,
    pub summary: MatchingSummary,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchableOrdersQuery {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
    pub search: Option<String>,
    pub sort_by: Option<OrderSort>,
    pub sort_direction: Option<SortDirection>,
}

#[derive(Clone, Debug, Serialize, sea_orm::FromQueryResult)]
#[serde(rename_all = "camelCase")]
pub struct MatchableOrderItem {
    pub id: String,
    pub external_order_number: Option<String>,
    pub paid_amount_fen: Option<i64>,
    pub ordered_at: Option<String>,
    pub shop_name: Option<String>,
    pub product_summary: String,
    pub matched_invoice_id: Option<String>,
    pub disabled_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchableOrdersPage {
    pub items: Vec<MatchableOrderItem>,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmInvoiceMatchRequest {
    pub invoice_id: String,
    pub order_ids: Vec<String>,
    pub source: String,
    pub accept_mismatches: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmInvoiceMatchesRequest {
    pub matches: Vec<ConfirmInvoiceMatchRequest>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmInvoiceMatchResult {
    pub invoice_id: String,
    pub confirmed: bool,
    pub differences: Vec<String>,
}
