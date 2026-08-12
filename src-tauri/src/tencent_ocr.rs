use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::{DateTime, NaiveDate, Utc};
use hmac::{Hmac, Mac};
use reqwest::StatusCode;
use rust_decimal::Decimal;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::error::{AppError, AppResult};
use crate::invoice_db::reliable_identity;
use crate::models::{NormalizedInvoice, NormalizedInvoiceItem, PrivateAppSettings};

type HmacSha256 = Hmac<Sha256>;
const HOST: &str = "ocr.tencentcloudapi.com";
const SERVICE: &str = "ocr";
const ACTION: &str = "RecognizeGeneralInvoice";
const VERSION: &str = "2018-11-19";

#[derive(Clone)]
pub struct TencentOcrClient {
    client: reqwest::Client,
    next_request: std::sync::Arc<Mutex<Instant>>,
}

pub struct TencentOcrResult {
    pub request_id: String,
    pub raw_response: String,
    pub invoices: Vec<NormalizedInvoice>,
    pub requires_review: bool,
    pub warning: Option<String>,
}

impl TencentOcrClient {
    pub fn new() -> AppResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(45))
            .build()
            .map_err(|error| AppError::Internal(error.to_string()))?;
        Ok(Self {
            client,
            next_request: std::sync::Arc::new(Mutex::new(Instant::now())),
        })
    }

    pub async fn recognize(
        &self,
        bytes: &[u8],
        forced_type: Option<i32>,
        settings: &PrivateAppSettings,
    ) -> AppResult<TencentOcrResult> {
        if settings.tencent_secret_id.is_empty() || settings.tencent_secret_key.is_empty() {
            return Err(AppError::InvalidInput(
                "请先在设置中配置腾讯云 SecretId 和 SecretKey".into(),
            ));
        }
        let mut body = serde_json::json!({
            "ImageBase64": STANDARD.encode(bytes), "EnableOther": true,
            "EnablePdf": true, "EnableMultiplePage": true, "EnableCutImage": true,
        });
        if let Some(value) = forced_type {
            body["Types"] = serde_json::json!([value]);
        }
        let payload =
            serde_json::to_string(&body).map_err(|error| AppError::Internal(error.to_string()))?;

        let mut last_error = None;
        for attempt in 0..3 {
            self.wait_for_rate_limit().await;
            match self.send(&payload, settings).await {
                Ok(response) => match parse_response(response) {
                    Ok(parsed) => return Ok(parsed),
                    Err(error) if is_retryable(&error) && attempt < 2 => {
                        last_error = Some(error);
                        tokio::time::sleep(Duration::from_millis(500 * (1 << attempt))).await;
                    }
                    Err(error) => return Err(error),
                },
                Err(error) if is_retryable(&error) && attempt < 2 => {
                    last_error = Some(error);
                    tokio::time::sleep(Duration::from_millis(500 * (1 << attempt))).await;
                }
                Err(error) => return Err(error),
            }
        }
        Err(last_error.unwrap_or_else(|| AppError::TencentOcr("请求失败".into())))
    }

    async fn wait_for_rate_limit(&self) {
        let mut next = self.next_request.lock().await;
        let now = Instant::now();
        if *next > now {
            tokio::time::sleep(*next - now).await;
        }
        *next = Instant::now() + Duration::from_millis(200);
    }

    async fn send(&self, payload: &str, settings: &PrivateAppSettings) -> AppResult<String> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| AppError::Internal(error.to_string()))?
            .as_secs();
        let date = DateTime::<Utc>::from_timestamp(timestamp as i64, 0)
            .ok_or_else(|| AppError::Internal("系统时间超出支持范围".into()))?
            .format("%Y-%m-%d")
            .to_string();
        let authorization = sign(
            &settings.tencent_secret_id,
            &settings.tencent_secret_key,
            timestamp,
            &date,
            payload,
        )?;
        let response = self
            .client
            .post(format!("https://{HOST}"))
            .header("Authorization", authorization)
            .header("Content-Type", "application/json; charset=utf-8")
            .header("Host", HOST)
            .header("X-TC-Action", ACTION)
            .header("X-TC-Timestamp", timestamp.to_string())
            .header("X-TC-Version", VERSION)
            .body(payload.to_owned())
            .send()
            .await
            .map_err(|error| AppError::TencentOcr(format!("网络请求失败：{error}")))?;
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|error| AppError::TencentOcr(format!("无法读取响应：{error}")))?;
        if !status.is_success() {
            return Err(http_error(status, &text));
        }
        Ok(text)
    }
}

pub fn sign(
    secret_id: &str,
    secret_key: &str,
    timestamp: u64,
    date: &str,
    payload: &str,
) -> AppResult<String> {
    let hashed_payload = hex_sha256(payload.as_bytes());
    let canonical_request = format!(
        "POST\n/\n\ncontent-type:application/json; charset=utf-8\nhost:{HOST}\n\ncontent-type;host\n{hashed_payload}"
    );
    let credential_scope = format!("{date}/{SERVICE}/tc3_request");
    let string_to_sign = format!(
        "TC3-HMAC-SHA256\n{timestamp}\n{credential_scope}\n{}",
        hex_sha256(canonical_request.as_bytes())
    );
    let secret_date = hmac(format!("TC3{secret_key}").as_bytes(), date.as_bytes())?;
    let secret_service = hmac(&secret_date, SERVICE.as_bytes())?;
    let secret_signing = hmac(&secret_service, b"tc3_request")?;
    let signature = hex::encode(hmac(&secret_signing, string_to_sign.as_bytes())?);
    Ok(format!(
        "TC3-HMAC-SHA256 Credential={secret_id}/{credential_scope}, SignedHeaders=content-type;host, Signature={signature}"
    ))
}

fn hmac(key: &[u8], data: &[u8]) -> AppResult<Vec<u8>> {
    let mut mac =
        HmacSha256::new_from_slice(key).map_err(|error| AppError::Internal(error.to_string()))?;
    mac.update(data);
    Ok(mac.finalize().into_bytes().to_vec())
}
fn hex_sha256(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

fn http_error(status: StatusCode, text: &str) -> AppError {
    if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
        AppError::TencentOcr(format!("temporary HTTP {status}"))
    } else {
        AppError::TencentOcr(format!("HTTP {status}: {}", safe_api_message(text)))
    }
}
fn is_retryable(error: &AppError) -> bool {
    matches!(error,AppError::TencentOcr(message) if message.contains("temporary")||message.contains("网络请求失败"))
}
fn safe_api_message(text: &str) -> String {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|v| {
            v.pointer("/Response/Error/Message")
                .and_then(|v| v.as_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "腾讯云返回错误".into())
}

pub fn parse_response(raw: String) -> AppResult<TencentOcrResult> {
    let root: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|error| AppError::InvalidOcr(format!("响应 JSON 无效：{error}")))?;
    let response = root
        .get("Response")
        .ok_or_else(|| AppError::InvalidOcr("响应缺少 Response".into()))?;
    if let Some(error) = response.get("Error") {
        let code = error
            .get("Code")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown");
        let message = error
            .get("Message")
            .and_then(|v| v.as_str())
            .unwrap_or("识别失败");
        let prefix = if code.contains("RequestLimitExceeded")
            || code.contains("InternalError")
            || code.contains("UnknownError")
        {
            "temporary "
        } else {
            ""
        };
        return Err(AppError::TencentOcr(format!("{prefix}{code}: {message}")));
    }
    let request_id = response
        .get("RequestId")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_owned();
    let items = response
        .get("MixedInvoiceItems")
        .and_then(|v| v.as_array())
        .ok_or_else(|| AppError::InvalidOcr("响应缺少 MixedInvoiceItems".into()))?;
    let failures = items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let code = item.get("Code").and_then(|v| v.as_str()).unwrap_or("OK");
            (code != "OK").then(|| format!("第 {} 个识别项：{code}", index + 1))
        })
        .collect::<Vec<_>>();
    let mut recognized = items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.get("Code").and_then(|v| v.as_str()).unwrap_or("OK") == "OK")
        .map(|(index, item)| normalize_invoice(item, index))
        .collect::<AppResult<Vec<_>>>()?;
    recognized.sort_by_key(|invoice| invoice.page_start);
    let mut invoices = Vec::<NormalizedInvoice>::new();
    let mut identities = HashMap::<String, usize>::new();
    let mut ambiguous_identity = false;
    for value in recognized {
        if let Some(identity) = value.reliable_identity.clone()
            && let Some(index) = identities.get(&identity).copied()
        {
            if value.page_start == invoices[index].page_end + 1 {
                merge_invoice(&mut invoices[index], value);
                continue;
            }
            ambiguous_identity = true;
        }
        let index = invoices.len();
        if let Some(identity) = value.reliable_identity.clone() {
            identities.insert(identity, index);
        }
        invoices.push(value);
    }
    let requires_review = ambiguous_identity
        || invoices.windows(2).any(|pair| {
            pair[0].reliable_identity.is_none()
                && pair[1].reliable_identity.is_none()
                && pair[0].subtype == pair[1].subtype
                && pair[1].page_start == pair[0].page_end + 1
        });
    if invoices.is_empty() {
        let detail = if failures.is_empty() {
            "未识别到可入库发票".to_owned()
        } else {
            format!("未识别到可入库发票（{}）", failures.join("；"))
        };
        return Err(AppError::InvalidOcr(detail));
    }
    let warning =
        (!failures.is_empty()).then(|| format!("部分票据未能识别：{}", failures.join("；")));
    Ok(TencentOcrResult {
        request_id,
        raw_response: raw,
        invoices,
        requires_review,
        warning,
    })
}

fn normalize_invoice(item: &serde_json::Value, _index: usize) -> AppResult<NormalizedInvoice> {
    let subtype = item
        .get("SubType")
        .and_then(|v| v.as_str())
        .unwrap_or("OtherInvoice")
        .to_owned();
    let info = item
        .pointer(&format!("/SingleInvoiceInfos/{subtype}"))
        .filter(|v| !v.is_null())
        .cloned()
        .or_else(|| {
            item.get("SingleInvoiceInfos")?
                .as_object()?
                .values()
                .find(|value| value.is_object())
                .cloned()
        })
        .unwrap_or_else(|| serde_json::json!({}));
    let code = field(&info, &["Code", "InvoiceCode", "InvoiceCodeConfirm"]);
    let number = field(
        &info,
        &[
            "Number",
            "InvoiceNumber",
            "InvoiceNum",
            "ElectronicFullNumber",
            "TicketNumber",
            "TicketNo",
            "ElectronicTicketNum",
            "BillNumber",
            "ReceiptNumber",
            "SerialNumber",
        ],
    );
    let page = item.get("Page").and_then(|v| v.as_i64()).unwrap_or(1) as i32;
    let raw_invoice_json =
        serde_json::to_string(item).map_err(|error| AppError::InvalidOcr(error.to_string()))?;
    let item_values = [
        "VatInvoiceItemInfos",
        "Items",
        "InvoiceItems",
        "TicketItems",
        "GoodsInfos",
        "GoodsInfo",
        "ProductInfos",
        "DetailInfos",
    ]
    .iter()
    .find_map(|key| info.get(*key).and_then(|v| v.as_array()))
    .or_else(|| {
        info.as_object()?.values().find_map(|value| {
            value
                .as_array()
                .filter(|values| values.first().is_some_and(serde_json::Value::is_object))
        })
    })
    .cloned()
    .unwrap_or_default();
    let items = item_values
        .into_iter()
        .map(|value| normalize_line(&value))
        .collect();
    Ok(NormalizedInvoice {
        id: uuid::Uuid::now_v7().to_string(),
        type_id: item.get("Type").and_then(|v| v.as_i64()).unwrap_or(-1) as i32,
        subtype: subtype.clone(),
        type_description: item
            .get("TypeDescription")
            .and_then(|v| v.as_str())
            .unwrap_or("发票")
            .to_owned(),
        subtype_description: item
            .get("SubTypeDescription")
            .and_then(|v| v.as_str())
            .unwrap_or(&subtype)
            .to_owned(),
        invoice_code: code.clone(),
        invoice_number: number.clone(),
        invoice_date: field(
            &info,
            &[
                "Date",
                "InvoiceDate",
                "IssueDate",
                "DateOfIssue",
                "TravelDate",
                "TransactionDate",
            ],
        )
        .and_then(|value| normalize_date(&value)),
        buyer_name: field(&info, &["Buyer", "BuyerName", "Purchaser", "PayerName"]),
        buyer_tax_id: field(
            &info,
            &["BuyerTaxID", "BuyerTaxId", "PurchaserTaxID", "PayerTaxID"],
        ),
        seller_name: field(
            &info,
            &["Seller", "SellerName", "PayeeName", "MerchantName"],
        ),
        seller_tax_id: field(
            &info,
            &["SellerTaxID", "SellerTaxId", "PayeeTaxID", "MerchantTaxID"],
        ),
        amount_fen: money(&info, &["PretaxAmount", "Amount", "Subtotal"]),
        tax_fen: money(&info, &["Tax", "TaxAmount"]),
        total_fen: money(
            &info,
            &[
                "Total",
                "TotalAmount",
                "Fare",
                "ReceiptMoney",
                "PaymentAmount",
            ],
        ),
        currency: field(&info, &["Currency", "CurrencyCode"]).unwrap_or_else(|| "CNY".into()),
        reliable_identity: reliable_identity(&subtype, code.as_deref(), number.as_deref()),
        possible_duplicate: false,
        preview_path: String::new(),
        page_start: page,
        page_end: page,
        raw_invoice_json,
        items,
    })
}
fn normalize_line(value: &serde_json::Value) -> NormalizedInvoiceItem {
    NormalizedInvoiceItem {
        name: field(value, &["Name", "ItemName", "GoodsName"]).unwrap_or_else(|| "明细".into()),
        specification: field(value, &["Specification", "Spec"]),
        unit: field(value, &["Unit"]),
        quantity: field(value, &["Quantity"]),
        unit_price_fen: money(value, &["Price", "UnitPrice"]),
        amount_fen: money(value, &["Total", "Amount"]),
        tax_rate: field(value, &["TaxRate"]),
        tax_fen: money(value, &["Tax", "TaxAmount"]),
        raw_json: serde_json::to_string(value).unwrap_or_else(|_| "{}".into()),
    }
}
fn field(value: &serde_json::Value, names: &[&str]) -> Option<String> {
    for name in names {
        if let Some(value) = value.get(*name) {
            let text = match value {
                serde_json::Value::String(text) => Some(text.trim().to_owned()),
                serde_json::Value::Number(number) => Some(number.to_string()),
                _ => None,
            };
            if let Some(text) = text.filter(|text| !text.is_empty()) {
                return Some(text);
            }
        }
    }
    None
}
fn money(value: &serde_json::Value, names: &[&str]) -> Option<i64> {
    let text = field(value, names)?;
    let cleaned = text.replace([',', '￥', '¥'], "");
    let decimal = cleaned.parse::<Decimal>().ok()?;
    (decimal * Decimal::from(100))
        .round()
        .to_string()
        .parse()
        .ok()
}
fn normalize_date(value: &str) -> Option<String> {
    let normalized = value
        .trim()
        .replace(['年', '月', '.', '/'], "-")
        .replace('日', "");
    for format in ["%Y-%m-%d", "%Y-%-m-%-d"] {
        if let Ok(date) = NaiveDate::parse_from_str(&normalized, format) {
            return Some(date.format("%Y-%m-%d").to_string());
        }
    }
    None
}
fn merge_invoice(target: &mut NormalizedInvoice, source: NormalizedInvoice) {
    target.page_start = target.page_start.min(source.page_start);
    target.page_end = target.page_end.max(source.page_end);
    target.items.extend(source.items);
    let left =
        serde_json::from_str::<serde_json::Value>(&target.raw_invoice_json).unwrap_or_default();
    let right =
        serde_json::from_str::<serde_json::Value>(&source.raw_invoice_json).unwrap_or_default();
    let mut values = match left {
        serde_json::Value::Array(values) => values,
        value => vec![value],
    };
    match right {
        serde_json::Value::Array(more) => values.extend(more),
        value => values.push(value),
    }
    target.raw_invoice_json = serde_json::Value::Array(values).to_string();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produces_stable_tc3_authorization() {
        let authorization = sign(
            "AKID-test-id",
            "test-secret-key",
            1_551_113_065,
            "2019-02-25",
            "{\"ImageBase64\":\"YWJj\"}",
        )
        .unwrap();
        assert_eq!(
            authorization,
            "TC3-HMAC-SHA256 Credential=AKID-test-id/2019-02-25/ocr/tc3_request, SignedHeaders=content-type;host, Signature=4558e9cea5b155eb7ef145902f9f0684f34e76911d51469f9d8b263b14cafd1c"
        );
    }

    #[tokio::test]
    async fn spaces_requests_to_at_most_five_per_second() {
        let client = TencentOcrClient::new().unwrap();
        client.wait_for_rate_limit().await;
        let started = Instant::now();
        client.wait_for_rate_limit().await;
        assert!(started.elapsed() >= Duration::from_millis(190));
    }

    #[test]
    fn parses_mixed_invoice_response() {
        let raw=serde_json::json!({"Response":{"RequestId":"req","MixedInvoiceItems":[{"Code":"OK","Type":3,"SubType":"VatSpecialInvoice","TypeDescription":"增值税发票","SubTypeDescription":"增值税专用发票","Page":1,"SingleInvoiceInfos":{"VatSpecialInvoice":{"Code":"4403","Number":"1234","Date":"2026-08-12","Buyer":"甲方","Seller":"乙方","PretaxAmount":"10.00","Tax":"0.60","Total":"10.60","VatInvoiceItemInfos":[{"Name":"服务","Total":"10.00"}]}}}]}}).to_string();
        let parsed = parse_response(raw).unwrap();
        assert_eq!(parsed.invoices[0].total_fen, Some(1060));
        assert_eq!(parsed.invoices[0].items.len(), 1);
        assert!(parsed.invoices[0].reliable_identity.is_some());
        assert!(parsed.warning.is_none());
    }

    #[test]
    fn preserves_partial_failures_as_a_task_warning() {
        let raw = serde_json::json!({"Response":{
            "RequestId":"req",
            "MixedInvoiceItems":[
                {"Code":"OK","Type":0,"SubType":"TaxiTicket","Page":1,
                 "SingleInvoiceInfos":{"TaxiTicket":{"InvoiceNum":"1024","Date":"2026-08-12","Fare":35.5}}},
                {"Code":"FailedOperation.ImageBlur","Page":2}
            ]
        }})
        .to_string();
        let parsed = parse_response(raw).unwrap();
        assert_eq!(parsed.invoices.len(), 1);
        assert_eq!(parsed.invoices[0].invoice_number.as_deref(), Some("1024"));
        assert_eq!(parsed.invoices[0].total_fen, Some(3550));
        assert!(
            parsed
                .warning
                .as_deref()
                .is_some_and(|warning| warning.contains("FailedOperation.ImageBlur"))
        );
    }

    #[test]
    fn falls_back_to_the_non_null_invoice_info_object() {
        let raw = serde_json::json!({"Response":{
            "RequestId":"req",
            "MixedInvoiceItems":[{
                "Code":"OK","Type":20,"SubType":"UnexpectedBankSubtype","Page":1,
                "SingleInvoiceInfos":{
                    "UnexpectedBankSubtype":null,
                    "BankSlip":{"ReceiptNumber":9001,"TransactionDate":"2026/8/2","PaymentAmount":"88.20"}
                }
            }]
        }})
        .to_string();
        let parsed = parse_response(raw).unwrap();
        assert_eq!(parsed.invoices[0].invoice_number.as_deref(), Some("9001"));
        assert_eq!(
            parsed.invoices[0].invoice_date.as_deref(),
            Some("2026-08-02")
        );
        assert_eq!(parsed.invoices[0].total_fen, Some(8820));
    }

    #[test]
    fn only_auto_merges_adjacent_pages_with_the_same_identity() {
        let item = |page| {
            serde_json::json!({
                "Code":"OK", "Type":3, "SubType":"VatSpecialInvoice", "Page":page,
                "SingleInvoiceInfos":{"VatSpecialInvoice":{"Code":"4403","Number":"1234"}}
            })
        };
        let adjacent = serde_json::json!({"Response":{"RequestId":"req","MixedInvoiceItems":[item(1), item(2)]}}).to_string();
        let parsed = parse_response(adjacent).unwrap();
        assert_eq!(parsed.invoices.len(), 1);
        assert_eq!(parsed.invoices[0].page_end, 2);
        assert!(!parsed.requires_review);

        let separated = serde_json::json!({"Response":{"RequestId":"req","MixedInvoiceItems":[item(1), item(3)]}}).to_string();
        let parsed = parse_response(separated).unwrap();
        assert_eq!(parsed.invoices.len(), 2);
        assert!(parsed.requires_review);
    }
}
