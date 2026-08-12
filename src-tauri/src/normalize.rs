use chrono::NaiveDateTime;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::{Decimal, RoundingStrategy};

use crate::error::{AppError, AppResult};
use crate::models::{NormalizedOrder, OcrOrder};

const DATE_FORMATS: &[&str] = &[
    "%Y-%m-%d %H:%M",
    "%Y-%m-%d %H:%M:%S",
    "%Y年%m月%d日 %H:%M",
    "%Y/%m/%d %H:%M",
];

pub fn normalize_ocr(value: OcrOrder, raw_json: String) -> AppResult<NormalizedOrder> {
    let mut warnings = Vec::new();

    let paid_amount_fen = match value.paid_amount {
        Some(number) => match parse_amount_to_fen(&number.to_string()) {
            Ok(amount) => Some(amount),
            Err(error) => {
                warnings.push(error.to_string());
                None
            }
        },
        None => None,
    };

    let ordered_at = value
        .ordered_at
        .and_then(|date| match normalize_datetime(&date) {
            Some(normalized) => Some(normalized),
            None => {
                warnings.push(format!("无法识别下单时间：{date}"));
                None
            }
        });

    let product_names = value
        .product_names
        .into_iter()
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .collect();

    Ok(NormalizedOrder {
        order_number: clean_optional(value.order_number),
        product_names,
        paid_amount_fen,
        ordered_at,
        shop_name: clean_optional(value.shop_name),
        warning: (!warnings.is_empty()).then(|| warnings.join("；")),
        raw_json,
    })
}

pub fn parse_amount_to_fen(value: &str) -> AppResult<i64> {
    let decimal = value
        .parse::<Decimal>()
        .map_err(|_| AppError::InvalidOcr(format!("无法识别实付金额：{value}")))?;
    if decimal.is_sign_negative() {
        return Err(AppError::InvalidOcr("实付金额不能为负数".into()));
    }
    (decimal * Decimal::from(100))
        .round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero)
        .to_i64()
        .ok_or_else(|| AppError::InvalidOcr("实付金额超出可存储范围".into()))
}

pub fn normalize_datetime(value: &str) -> Option<String> {
    let value = value.trim();
    DATE_FORMATS.iter().find_map(|format| {
        NaiveDateTime::parse_from_str(value, format)
            .ok()
            .map(|date| date.format("%Y-%m-%d %H:%M").to_string())
    })
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value.and_then(|text| {
        let text = text.trim().to_owned();
        (!text.is_empty()).then_some(text)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_decimal_amount_to_fen_without_float_error() {
        assert_eq!(parse_amount_to_fen("19.99").unwrap(), 1999);
        assert_eq!(parse_amount_to_fen("0.005").unwrap(), 1);
    }

    #[test]
    fn normalizes_supported_taobao_dates() {
        assert_eq!(
            normalize_datetime("2026年08月12日 09:30").as_deref(),
            Some("2026-08-12 09:30")
        );
        assert_eq!(normalize_datetime("not a date"), None);
    }
}
