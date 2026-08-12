use serde::{Deserialize, Serialize};

pub const DEFAULT_MAX_CONCURRENCY: u8 = 3;
pub const MIN_CONCURRENCY: u8 = 1;
pub const MAX_CONCURRENCY: u8 = 8;
pub const DEFAULT_INVOICE_OCR_CONCURRENCY: u8 = 3;
pub const MAX_INVOICE_OCR_CONCURRENCY: u8 = 5;

#[derive(Clone, Debug, Deserialize, Serialize, sea_orm::FromQueryResult)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub ollama_base_url: String,
    pub ollama_model: String,
    pub max_concurrency: u8,
    pub tencent_secret_id_masked: Option<String>,
    pub tencent_credentials_configured: bool,
    pub invoice_ocr_concurrency: u8,
    pub settings_path: String,
    pub permission_warning: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsRequest {
    pub ollama_base_url: String,
    pub ollama_model: String,
    pub max_concurrency: u8,
    pub tencent_secret_id: Option<String>,
    pub tencent_secret_key: Option<String>,
    pub clear_tencent_credentials: bool,
    pub invoice_ocr_concurrency: u8,
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
        if !(MIN_CONCURRENCY..=MAX_INVOICE_OCR_CONCURRENCY).contains(&self.invoice_ocr_concurrency)
        {
            return Err(crate::error::AppError::InvalidInput(
                "腾讯云 OCR 并发数必须在 1 到 5 之间".into(),
            ));
        }
        Ok(AppSettings {
            ollama_base_url: base_url.to_owned(),
            ollama_model: model.to_owned(),
            max_concurrency: self.max_concurrency,
            tencent_secret_id_masked: None,
            tencent_credentials_configured: false,
            invoice_ocr_concurrency: self.invoice_ocr_concurrency,
            settings_path: String::new(),
            permission_warning: None,
        })
    }
}

#[derive(Clone, Debug)]
pub struct PrivateAppSettings {
    pub ollama_base_url: String,
    pub ollama_model: String,
    pub max_concurrency: u8,
    pub tencent_secret_id: String,
    pub tencent_secret_key: String,
    pub invoice_ocr_concurrency: u8,
    pub settings_path: String,
    pub permission_warning: Option<String>,
}

impl PrivateAppSettings {
    pub fn public(&self) -> AppSettings {
        AppSettings {
            ollama_base_url: self.ollama_base_url.clone(),
            ollama_model: self.ollama_model.clone(),
            max_concurrency: self.max_concurrency,
            tencent_secret_id_masked: mask_secret_id(&self.tencent_secret_id),
            tencent_credentials_configured: !self.tencent_secret_id.is_empty()
                && !self.tencent_secret_key.is_empty(),
            invoice_ocr_concurrency: self.invoice_ocr_concurrency,
            settings_path: self.settings_path.clone(),
            permission_warning: self.permission_warning.clone(),
        }
    }

    pub fn ollama(&self) -> AppSettings {
        self.public()
    }
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

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportBatchesQuery {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

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
pub struct TaskUpdateEvent {
    pub task: ImportTask,
    pub batch: BatchDetail,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapState {
    pub settings: AppSettings,
    pub latest_batch: Option<BatchDetail>,
    pub latest_invoice_batch: Option<InvoiceBatchDetail>,
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
