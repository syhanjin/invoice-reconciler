use std::collections::{HashMap, HashSet};

use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseTransaction, DbBackend, EntityTrait, FromQueryResult,
    QueryFilter, Set, Statement, TransactionTrait,
};
use tauri::State;

use crate::AppState;
use crate::db::{Database, now};
use crate::entities::{invoice, invoice_match, invoice_match_order, order};
use crate::error::{AppError, AppResult, CommandResult};
use crate::models::{
    ConfirmInvoiceMatchRequest, ConfirmInvoiceMatchResult, ConfirmInvoiceMatchesRequest, IdRequest,
    MatchCandidate, MatchOrderSummary, MatchSummariesPage, MatchSummariesQuery,
    MatchSummaryAttemptsPage, MatchSummaryAttemptsQuery, MatchSummaryCounts, MatchableOrderItem,
    MatchableOrdersPage, MatchableOrdersQuery, MatchingInvoiceItem, MatchingPage, MatchingQuery,
    MatchingSummary, OrderSort, SortDirection, UpdateMatchSummaryRequest,
};

#[derive(Clone, Debug, FromQueryResult)]
struct InvoiceRow {
    id: String,
    invoice_number: Option<String>,
    subtype: String,
    subtype_description: String,
    invoice_date: Option<String>,
    seller_name: Option<String>,
    total_fen: Option<i64>,
    currency: String,
    confirmed_source: Option<String>,
    confirmed_at: Option<String>,
    display_name: Option<String>,
    summary_status: Option<String>,
    summary_source: Option<String>,
    summary_error: Option<String>,
    summary_provider: Option<String>,
    summary_model: Option<String>,
    summary_generated_at: Option<String>,
}

#[derive(Clone, Debug, FromQueryResult)]
struct OrderRow {
    id: String,
    external_order_number: Option<String>,
    paid_amount_fen: i64,
    ordered_at: Option<String>,
    shop_name: Option<String>,
    product_summary: String,
}

struct ConfirmedOrders {
    candidates: HashMap<String, MatchCandidate>,
    product_names: HashMap<String, Vec<String>>,
}

pub(crate) struct LedgerMatchProjection {
    pub invoice_statuses: HashMap<String, String>,
    pub order_statuses: HashMap<String, String>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct RepurchaseInvoiceKey {
    total_fen: i64,
    seller_name: Option<String>,
    subtype: String,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct RepurchaseOrderKey {
    paid_amount_fen: i64,
    shop_name: Option<String>,
}

impl OrderRow {
    fn summary(&self) -> MatchOrderSummary {
        MatchOrderSummary {
            id: self.id.clone(),
            external_order_number: self.external_order_number.clone(),
            paid_amount_fen: self.paid_amount_fen,
            ordered_at: self.ordered_at.clone(),
            shop_name: self.shop_name.clone(),
            product_summary: self.product_summary.clone(),
        }
    }
}

impl Database {
    async fn matching_items(
        &self,
        max_orders: usize,
        max_candidates: usize,
    ) -> AppResult<Vec<MatchingInvoiceItem>> {
        let invoices = InvoiceRow::find_by_statement(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT i.id,i.invoice_number,i.subtype,i.subtype_description,i.invoice_date,i.seller_name,\
             i.total_fen,i.currency,m.source confirmed_source,m.confirmed_at,m.display_name,\
             m.summary_status,m.summary_source,m.summary_error,m.summary_provider,m.summary_model,m.summary_generated_at \
             FROM invoices i LEFT JOIN invoice_matches m ON m.invoice_id=i.id \
             LEFT JOIN project_invoices pi ON pi.invoice_id=i.id \
             LEFT JOIN projects p ON p.id=pi.project_id \
             WHERE p.archived_at IS NULL \
             ORDER BY COALESCE(i.invoice_date,'' ) DESC,i.id DESC",
        ))
        .all(&self.connection)
        .await?;
        let orders = OrderRow::find_by_statement(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT o.id,o.external_order_number,o.paid_amount_fen,o.ordered_at,o.shop_name,\
             COALESCE((SELECT group_concat(product_name,' · ') FROM \
               (SELECT product_name FROM order_items WHERE order_id=o.id ORDER BY position LIMIT 2)),'') product_summary \
             FROM orders o LEFT JOIN invoice_match_orders mo ON mo.order_id=o.id \
             WHERE mo.order_id IS NULL AND o.paid_amount_fen IS NOT NULL \
             ORDER BY o.id",
        ))
        .all(&self.connection)
        .await?;
        let confirmed_orders = confirmed_orders(&self.connection).await?;

        let mut candidates_by_invoice = HashMap::<String, (Vec<MatchCandidate>, bool)>::new();
        for value in &invoices {
            if value.confirmed_source.is_none()
                && value.total_fen.is_some_and(|amount| amount >= 0)
                && value.currency.eq_ignore_ascii_case("CNY")
            {
                candidates_by_invoice.insert(
                    value.id.clone(),
                    generate_candidates(
                        value.total_fen.unwrap_or_default(),
                        value.invoice_date.as_deref(),
                        &orders,
                        max_orders,
                        max_candidates,
                    ),
                );
            }
        }

        let repurchase_conflicts =
            resolve_repurchase_candidates(&invoices, &orders, &mut candidates_by_invoice);

        let mut order_invoice_uses = HashMap::<String, HashSet<String>>::new();
        for (invoice_id, (candidates, _)) in &candidates_by_invoice {
            for candidate in candidates {
                for order_id in &candidate.order_ids {
                    order_invoice_uses
                        .entry(order_id.clone())
                        .or_default()
                        .insert(invoice_id.clone());
                }
            }
        }

        let mut items = Vec::with_capacity(invoices.len());
        for value in invoices {
            let (status, reason, candidates) = if value.confirmed_source.is_some() {
                (
                    "confirmed".to_owned(),
                    None,
                    confirmed_orders
                        .candidates
                        .get(&value.id)
                        .cloned()
                        .into_iter()
                        .collect(),
                )
            } else if value.total_fen.is_none() {
                (
                    "ineligible".to_owned(),
                    Some("发票金额缺失，不参与匹配".to_owned()),
                    Vec::new(),
                )
            } else if value.total_fen.is_some_and(|amount| amount < 0) {
                (
                    "ineligible".to_owned(),
                    Some("发票金额无效，不参与匹配".to_owned()),
                    Vec::new(),
                )
            } else if !value.currency.eq_ignore_ascii_case("CNY") {
                (
                    "ineligible".to_owned(),
                    Some("仅人民币发票参与匹配".to_owned()),
                    Vec::new(),
                )
            } else {
                let (candidates, overflow) =
                    candidates_by_invoice.remove(&value.id).unwrap_or_default();
                if overflow {
                    (
                        "conflict".to_owned(),
                        Some(format!("候选组合超过 {max_candidates} 个，请自定义选择")),
                        Vec::new(),
                    )
                } else if let Some(reason) = repurchase_conflicts.get(&value.id) {
                    ("conflict".to_owned(), Some(reason.clone()), candidates)
                } else if candidates.is_empty() {
                    (
                        "unmatched".to_owned(),
                        Some("没有满足自动规则的订单组合".to_owned()),
                        candidates,
                    )
                } else {
                    let cross_invoice_conflict = candidates.iter().any(|candidate| {
                        candidate.order_ids.iter().any(|order_id| {
                            order_invoice_uses
                                .get(order_id)
                                .is_some_and(|invoice_ids| invoice_ids.len() > 1)
                        })
                    });
                    if candidates.len() > 1 || cross_invoice_conflict {
                        (
                            "conflict".to_owned(),
                            Some(if candidates.len() > 1 {
                                "存在多个订单组合，需要手动选择".to_owned()
                            } else {
                                "候选订单同时被其他发票需要，需要手动处理".to_owned()
                            }),
                            candidates,
                        )
                    } else {
                        ("suggested".to_owned(), None, candidates)
                    }
                }
            };
            let display_name = if status == "confirmed" {
                let candidate = candidates.first();
                let products = confirmed_orders
                    .product_names
                    .get(&value.id)
                    .cloned()
                    .unwrap_or_default();
                let labels = candidate
                    .into_iter()
                    .flat_map(|candidate| candidate.orders.iter())
                    .map(|order| {
                        order
                            .external_order_number
                            .clone()
                            .unwrap_or_else(|| order.id.clone())
                    })
                    .collect::<Vec<_>>();
                Some(crate::match_summary::display_name(
                    value.display_name.as_deref(),
                    &products,
                    &labels,
                ))
            } else {
                None
            };
            items.push(MatchingInvoiceItem {
                id: value.id,
                invoice_number: value.invoice_number,
                subtype_description: value.subtype_description,
                invoice_date: value.invoice_date,
                seller_name: value.seller_name,
                total_fen: value.total_fen,
                currency: value.currency,
                status,
                reason,
                candidates,
                confirmed_source: value.confirmed_source,
                confirmed_at: value.confirmed_at,
                display_name,
                summary_status: value.summary_status,
                summary_source: value.summary_source,
                summary_error: value.summary_error,
                summary_provider: value.summary_provider,
                summary_model: value.summary_model,
                summary_generated_at: value.summary_generated_at,
            });
        }
        Ok(items)
    }

    pub(crate) async fn ledger_match_projection(
        &self,
        max_orders: usize,
        max_candidates: usize,
    ) -> AppResult<LedgerMatchProjection> {
        let items = self.matching_items(max_orders, max_candidates).await?;
        let mut invoice_statuses = HashMap::with_capacity(items.len());
        let mut order_statuses = HashMap::new();
        for item in items {
            let ledger_status = match item.status.as_str() {
                "confirmed" => "confirmed",
                "suggested" => "matched",
                _ => "unmatched",
            };
            invoice_statuses.insert(item.id, ledger_status.to_owned());
            if matches!(ledger_status, "confirmed" | "matched") {
                for candidate in item.candidates {
                    for order_id in candidate.order_ids {
                        let status = order_statuses
                            .entry(order_id)
                            .or_insert_with(|| ledger_status.to_owned());
                        if ledger_status == "confirmed" {
                            *status = "confirmed".to_owned();
                        }
                    }
                }
            }
        }
        Ok(LedgerMatchProjection {
            invoice_statuses,
            order_statuses,
        })
    }

    pub async fn matching_page(
        &self,
        query: MatchingQuery,
        max_orders: usize,
        max_candidates: usize,
    ) -> AppResult<MatchingPage> {
        let mut items = self.matching_items(max_orders, max_candidates).await?;
        let mut summary = MatchingSummary::default();
        let mut summary_counts = MatchSummaryCounts::default();
        for item in &items {
            summary.total += 1;
            match item.status.as_str() {
                "suggested" => summary.suggested += 1,
                "conflict" => summary.conflict += 1,
                "unmatched" => summary.unmatched += 1,
                "ineligible" => summary.ineligible += 1,
                "confirmed" => summary.confirmed += 1,
                _ => {}
            }
            if let Some(status) = &item.summary_status {
                summary_counts.add(status);
            }
        }

        let search = query.search.unwrap_or_default().trim().to_lowercase();
        let status_filter = query.status.unwrap_or_default();
        let summary_status_filter = normalized_summary_filter(query.summary_status.as_deref())?;
        items.retain(|item| {
            (status_filter.is_empty() || status_filter == "all" || item.status == status_filter)
                && summary_status_filter.is_none_or(|status| {
                    item.status == "confirmed" && item.summary_status.as_deref() == Some(status)
                })
                && (search.is_empty()
                    || item.id.to_lowercase().contains(&search)
                    || item
                        .invoice_number
                        .as_deref()
                        .unwrap_or_default()
                        .to_lowercase()
                        .contains(&search)
                    || item
                        .seller_name
                        .as_deref()
                        .unwrap_or_default()
                        .to_lowercase()
                        .contains(&search)
                    || item
                        .display_name
                        .as_deref()
                        .unwrap_or_default()
                        .to_lowercase()
                        .contains(&search)
                    || item.subtype_description.to_lowercase().contains(&search))
        });
        let page = query.page.unwrap_or(1).max(1);
        let page_size = query.page_size.unwrap_or(50).clamp(1, 100);
        let total = i64::try_from(items.len()).unwrap_or(i64::MAX);
        let start = usize::try_from((page - 1) * page_size).unwrap_or(usize::MAX);
        let items = items
            .into_iter()
            .skip(start)
            .take(page_size as usize)
            .collect();
        Ok(MatchingPage {
            items,
            summary,
            summary_counts,
            total,
            page,
            page_size,
        })
    }

    pub async fn list_matchable_orders(
        &self,
        query: MatchableOrdersQuery,
    ) -> AppResult<MatchableOrdersPage> {
        let page = query.page.unwrap_or(1).max(1);
        let page_size = query.page_size.unwrap_or(20).clamp(1, 100);
        let search = query.search.unwrap_or_default().trim().to_owned();
        let mut filter = String::from(
            " AND NOT EXISTS (SELECT 1 FROM invoice_match_orders mo WHERE mo.order_id=o.id)",
        );
        let mut values = Vec::<sea_orm::Value>::new();
        if !search.is_empty() {
            filter.push_str(" AND (o.external_order_number LIKE ? OR o.shop_name LIKE ? OR EXISTS (SELECT 1 FROM order_items oi WHERE oi.order_id=o.id AND oi.product_name LIKE ?))");
            let pattern = format!("%{search}%");
            values.extend([
                pattern.clone().into(),
                pattern.clone().into(),
                pattern.into(),
            ]);
        }
        #[derive(FromQueryResult)]
        struct CountRow {
            count: i64,
        }
        let total = CountRow::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            format!("SELECT COUNT(*) count FROM orders o WHERE 1=1{filter}"),
            values.clone(),
        ))
        .one(&self.connection)
        .await?
        .map(|row| row.count)
        .unwrap_or(0);
        let sort = match query.sort_by.unwrap_or(OrderSort::CreatedAt) {
            OrderSort::CreatedAt => "o.created_at",
            OrderSort::OrderedAt => "o.ordered_at",
            OrderSort::PaidAmount => "o.paid_amount_fen",
            OrderSort::ShopName => "o.shop_name",
        };
        let direction = match query.sort_direction.unwrap_or(SortDirection::Desc) {
            SortDirection::Asc => "ASC",
            SortDirection::Desc => "DESC",
        };
        values.extend([
            i64::from(page_size).into(),
            i64::from((page - 1) * page_size).into(),
        ]);
        let items = MatchableOrderItem::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            format!(
                "SELECT o.id,o.external_order_number,o.paid_amount_fen,o.ordered_at,o.shop_name,\
                 COALESCE((SELECT group_concat(product_name,' · ') FROM (SELECT product_name FROM order_items WHERE order_id=o.id ORDER BY position LIMIT 2)),'') product_summary,\
                 NULL matched_invoice_id,CASE WHEN o.paid_amount_fen IS NULL THEN '订单金额缺失' WHEN o.paid_amount_fen < 0 THEN '订单金额无效' ELSE NULL END disabled_reason \
                 FROM orders o WHERE 1=1{filter} \
                 ORDER BY {sort} {direction},o.id DESC LIMIT ? OFFSET ?"
            ),
            values,
        ))
        .all(&self.connection)
        .await?;
        Ok(MatchableOrdersPage {
            items,
            total,
            page,
            page_size,
        })
    }

    #[cfg(test)]
    pub async fn confirm_invoice_match(
        &self,
        request: ConfirmInvoiceMatchRequest,
    ) -> AppResult<ConfirmInvoiceMatchResult> {
        self.confirm_invoice_match_with_summary_status(request, "pending")
            .await
    }

    pub async fn confirm_invoice_match_with_summary_status(
        &self,
        request: ConfirmInvoiceMatchRequest,
        summary_status: &str,
    ) -> AppResult<ConfirmInvoiceMatchResult> {
        let transaction = self.connection.begin().await?;
        let result = self
            .confirm_invoice_match_in(&transaction, request, summary_status)
            .await?;
        if result.confirmed {
            transaction.commit().await?;
        } else {
            transaction.rollback().await?;
        }
        Ok(result)
    }

    #[cfg(test)]
    pub async fn confirm_invoice_matches(
        &self,
        requests: Vec<ConfirmInvoiceMatchRequest>,
    ) -> AppResult<Vec<ConfirmInvoiceMatchResult>> {
        self.confirm_invoice_matches_with_summary_status(requests, "pending")
            .await
    }

    pub async fn confirm_invoice_matches_with_summary_status(
        &self,
        requests: Vec<ConfirmInvoiceMatchRequest>,
        summary_status: &str,
    ) -> AppResult<Vec<ConfirmInvoiceMatchResult>> {
        let transaction = self.connection.begin().await?;
        let mut results = Vec::with_capacity(requests.len());
        for request in requests {
            let result = self
                .confirm_invoice_match_in(&transaction, request, summary_status)
                .await?;
            if !result.confirmed {
                return Err(AppError::InvalidInput(
                    "批量匹配中出现需要人工确认的差异".into(),
                ));
            }
            results.push(result);
        }
        transaction.commit().await?;
        Ok(results)
    }

    async fn confirm_invoice_match_in(
        &self,
        transaction: &DatabaseTransaction,
        request: ConfirmInvoiceMatchRequest,
        summary_status: &str,
    ) -> AppResult<ConfirmInvoiceMatchResult> {
        if summary_status != "pending" && summary_status != "not_generated" {
            return Err(AppError::Internal("摘要初始状态无效".into()));
        }
        if request.order_ids.is_empty() || request.order_ids.len() > 100 {
            return Err(AppError::InvalidInput(
                "单次匹配请选择 1 到 100 张订单".into(),
            ));
        }
        if request.source != "automatic" && request.source != "custom" {
            return Err(AppError::InvalidInput("匹配来源无效".into()));
        }
        let mut seen = HashSet::new();
        if request.order_ids.iter().any(|id| !seen.insert(id.clone())) {
            return Err(AppError::InvalidInput("订单组合中存在重复订单".into()));
        }
        let invoice_value = invoice::Entity::find_by_id(&request.invoice_id)
            .one(transaction)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("发票 {}", request.invoice_id)))?;
        if invoice_match::Entity::find_by_id(&request.invoice_id)
            .one(transaction)
            .await?
            .is_some()
        {
            return Err(AppError::MatchingLocked(format!(
                "发票 {}",
                request.invoice_id
            )));
        }
        let total_fen = invoice_value
            .total_fen
            .ok_or_else(|| AppError::InvalidInput("发票金额缺失，不参与匹配".into()))?;
        if total_fen < 0 {
            return Err(AppError::InvalidInput("发票金额无效，不参与匹配".into()));
        }
        if !invoice_value.currency.eq_ignore_ascii_case("CNY") {
            return Err(AppError::InvalidInput("仅人民币发票参与匹配".into()));
        }
        let order_values = order::Entity::find()
            .filter(order::Column::Id.is_in(request.order_ids.clone()))
            .all(transaction)
            .await?;
        if order_values.len() != request.order_ids.len() {
            return Err(AppError::InvalidInput(
                "部分订单已不存在，请刷新后重试".into(),
            ));
        }
        let occupied = invoice_match_order::Entity::find()
            .filter(invoice_match_order::Column::OrderId.is_in(request.order_ids.clone()))
            .all(transaction)
            .await?;
        if !occupied.is_empty() {
            return Err(AppError::MatchingLocked("所选订单已被其他发票占用".into()));
        }
        let by_id = order_values
            .into_iter()
            .map(|value| (value.id.clone(), value))
            .collect::<HashMap<_, _>>();
        let ordered_values = request
            .order_ids
            .iter()
            .map(|id| by_id.get(id).expect("validated order id"))
            .collect::<Vec<_>>();
        if ordered_values.iter().any(|value| {
            value
                .paid_amount_fen
                .is_none_or(|paid_amount_fen| paid_amount_fen < 0)
        }) {
            return Err(AppError::InvalidInput(
                "金额缺失或无效的订单不能参与匹配".into(),
            ));
        }
        let sum = ordered_values
            .iter()
            .filter_map(|value| value.paid_amount_fen)
            .sum::<i64>();
        let differences = match_differences(
            total_fen,
            invoice_value.invoice_date.as_deref(),
            &ordered_values,
        );
        if !differences.is_empty() && request.source == "automatic" {
            return Err(AppError::InvalidInput(format!(
                "自动候选已失效：{}",
                differences.join("；")
            )));
        }
        if !differences.is_empty() && !request.accept_mismatches {
            return Ok(ConfirmInvoiceMatchResult {
                invoice_id: request.invoice_id,
                confirmed: false,
                differences,
            });
        }
        debug_assert_eq!(
            sum,
            ordered_values
                .iter()
                .map(|v| v.paid_amount_fen.unwrap_or_default())
                .sum::<i64>()
        );
        invoice_match::ActiveModel {
            invoice_id: Set(request.invoice_id.clone()),
            source: Set(request.source),
            confirmed_at: Set(now()),
            display_name: Set(None),
            summary_status: Set(summary_status.into()),
            summary_source: Set(None),
            summary_trigger: Set(
                (summary_status == "pending").then(|| "automatic_confirmation".into())
            ),
            summary_error: Set(None),
            summary_provider: Set(None),
            summary_model: Set(None),
            summary_input_hash: Set(None),
            summary_generated_at: Set(None),
        }
        .insert(transaction)
        .await?;
        for (position, order_id) in request.order_ids.into_iter().enumerate() {
            invoice_match_order::ActiveModel {
                invoice_id: Set(request.invoice_id.clone()),
                order_id: Set(order_id),
                position: Set(i32::try_from(position).unwrap_or(i32::MAX)),
            }
            .insert(transaction)
            .await?;
        }
        Ok(ConfirmInvoiceMatchResult {
            invoice_id: request.invoice_id,
            confirmed: true,
            differences,
        })
    }

    pub async fn unconfirm_invoice_match(&self, invoice_id: &str) -> AppResult<()> {
        let result = invoice_match::Entity::delete_by_id(invoice_id)
            .exec(&self.connection)
            .await?;
        if result.rows_affected == 0 {
            return Err(AppError::NotFound(format!("发票匹配 {invoice_id}")));
        }
        Ok(())
    }
}

fn normalized_summary_filter(value: Option<&str>) -> AppResult<Option<&str>> {
    match value
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "all")
    {
        None => Ok(None),
        Some(value @ ("not_generated" | "pending" | "processing" | "succeeded" | "failed")) => {
            Ok(Some(value))
        }
        Some(_) => Err(AppError::InvalidInput("摘要状态筛选无效".into())),
    }
}

async fn confirmed_orders(connection: &sea_orm::DatabaseConnection) -> AppResult<ConfirmedOrders> {
    #[derive(FromQueryResult)]
    struct ConfirmedOrderRow {
        invoice_id: String,
        id: String,
        external_order_number: Option<String>,
        paid_amount_fen: i64,
        ordered_at: Option<String>,
        shop_name: Option<String>,
        product_summary: String,
    }
    #[derive(FromQueryResult)]
    struct ConfirmedProductRow {
        invoice_id: String,
        product_name: String,
    }
    let rows = ConfirmedOrderRow::find_by_statement(Statement::from_string(
        DbBackend::Sqlite,
        "SELECT mo.invoice_id,o.id,o.external_order_number,o.paid_amount_fen,o.ordered_at,o.shop_name,\
         COALESCE((SELECT group_concat(product_name,' · ') FROM (SELECT product_name FROM order_items WHERE order_id=o.id ORDER BY position)),'') product_summary \
         FROM invoice_match_orders mo JOIN orders o ON o.id=mo.order_id ORDER BY mo.invoice_id,mo.position",
    ))
    .all(connection)
    .await?;
    let product_rows = ConfirmedProductRow::find_by_statement(Statement::from_string(
        DbBackend::Sqlite,
        "SELECT mo.invoice_id,oi.product_name FROM invoice_match_orders mo \
         JOIN order_items oi ON oi.order_id=mo.order_id \
         ORDER BY mo.invoice_id,mo.position,oi.position",
    ))
    .all(connection)
    .await?;
    let mut candidates = HashMap::<String, MatchCandidate>::new();
    for row in rows {
        let candidate = candidates
            .entry(row.invoice_id)
            .or_insert_with(|| MatchCandidate {
                order_ids: Vec::new(),
                orders: Vec::new(),
                total_fen: 0,
                shop_name: None,
            });
        candidate.order_ids.push(row.id.clone());
        candidate.total_fen += row.paid_amount_fen;
        candidate.orders.push(MatchOrderSummary {
            id: row.id,
            external_order_number: row.external_order_number,
            paid_amount_fen: row.paid_amount_fen,
            ordered_at: row.ordered_at,
            shop_name: row.shop_name,
            product_summary: row.product_summary,
        });
    }
    for candidate in candidates.values_mut() {
        candidate.shop_name = common_shop(&candidate.orders);
    }
    let mut product_names = HashMap::<String, Vec<String>>::new();
    for row in product_rows {
        product_names
            .entry(row.invoice_id)
            .or_default()
            .push(row.product_name);
    }
    Ok(ConfirmedOrders {
        candidates,
        product_names,
    })
}

fn resolve_repurchase_candidates(
    invoices: &[InvoiceRow],
    orders: &[OrderRow],
    candidates_by_invoice: &mut HashMap<String, (Vec<MatchCandidate>, bool)>,
) -> HashMap<String, String> {
    let eligible_invoices = invoices
        .iter()
        .filter(|invoice| {
            invoice.confirmed_source.is_none()
                && invoice.total_fen.is_some_and(|amount| amount >= 0)
                && invoice.currency.eq_ignore_ascii_case("CNY")
        })
        .collect::<Vec<_>>();
    let eligible_orders = orders
        .iter()
        .filter(|order| order.paid_amount_fen >= 0)
        .collect::<Vec<_>>();

    let mut invoice_groups = HashMap::<RepurchaseInvoiceKey, Vec<&InvoiceRow>>::new();
    for invoice in eligible_invoices {
        invoice_groups
            .entry(RepurchaseInvoiceKey {
                total_fen: invoice.total_fen.unwrap_or_default(),
                seller_name: normalized_shop(invoice.seller_name.as_deref()),
                subtype: invoice.subtype.clone(),
            })
            .or_default()
            .push(invoice);
    }
    let mut order_groups = HashMap::<RepurchaseOrderKey, Vec<&OrderRow>>::new();
    let mut order_keys_by_id = HashMap::<&str, RepurchaseOrderKey>::new();
    for order in eligible_orders {
        let key = RepurchaseOrderKey {
            paid_amount_fen: order.paid_amount_fen,
            shop_name: normalized_shop(order.shop_name.as_deref()),
        };
        order_keys_by_id.insert(&order.id, key.clone());
        order_groups.entry(key).or_default().push(order);
    }

    let mut invoice_order_links =
        HashMap::<RepurchaseInvoiceKey, HashSet<RepurchaseOrderKey>>::new();
    let mut order_invoice_links =
        HashMap::<RepurchaseOrderKey, HashSet<RepurchaseInvoiceKey>>::new();
    let mut disqualified_invoice_keys = HashSet::<RepurchaseInvoiceKey>::new();
    for (invoice_key, invoice_group) in &invoice_groups {
        for invoice in invoice_group {
            let Some((candidates, overflow)) = candidates_by_invoice.get(&invoice.id) else {
                continue;
            };
            if *overflow
                || candidates
                    .iter()
                    .any(|candidate| candidate.order_ids.len() != 1)
            {
                disqualified_invoice_keys.insert(invoice_key.clone());
            }
            for candidate in candidates {
                for order_id in &candidate.order_ids {
                    let Some(order_key) = order_keys_by_id.get(order_id.as_str()) else {
                        continue;
                    };
                    invoice_order_links
                        .entry(invoice_key.clone())
                        .or_default()
                        .insert(order_key.clone());
                    order_invoice_links
                        .entry(order_key.clone())
                        .or_default()
                        .insert(invoice_key.clone());
                }
            }
        }
    }

    let mut conflicts = HashMap::new();
    for (invoice_key, invoice_group) in invoice_groups {
        if invoice_group.len() < 2 || disqualified_invoice_keys.contains(&invoice_key) {
            continue;
        }
        let Some(linked_order_keys) = invoice_order_links.get(&invoice_key) else {
            continue;
        };
        if linked_order_keys.len() != 1 {
            continue;
        }
        let order_key = linked_order_keys
            .iter()
            .next()
            .expect("one linked order group");
        if order_invoice_links
            .get(order_key)
            .is_none_or(|keys| keys.len() != 1 || !keys.contains(&invoice_key))
        {
            continue;
        }
        let Some(order_group) = order_groups.get(order_key) else {
            continue;
        };
        if order_group.len() < 2 {
            continue;
        }
        if invoice_group.len() != order_group.len() {
            let reason = format!(
                "疑似回购冲突：订单 {} 条，发票 {} 张，数量不一致",
                order_group.len(),
                invoice_group.len()
            );
            for invoice in invoice_group {
                conflicts.insert(invoice.id.clone(), reason.clone());
            }
            continue;
        }

        let Some(sorted_invoices) = sorted_complete_dates(
            &invoice_group,
            |invoice| invoice.invoice_date.as_deref(),
            |invoice| invoice.id.as_str(),
        ) else {
            mark_repurchase_missing_date_conflict(&mut conflicts, &invoice_group);
            continue;
        };
        let Some(sorted_orders) = sorted_complete_dates(
            order_group,
            |order| order.ordered_at.as_deref(),
            |order| order.id.as_str(),
        ) else {
            mark_repurchase_missing_date_conflict(&mut conflicts, &invoice_group);
            continue;
        };
        if sorted_invoices
            .iter()
            .zip(&sorted_orders)
            .any(|(invoice, order)| {
                !date_allowed(order.ordered_at.as_deref(), invoice.invoice_date.as_deref())
            })
        {
            let reason = "疑似回购冲突：按时间配对后存在订单晚于开票日期".to_owned();
            for invoice in invoice_group {
                conflicts.insert(invoice.id.clone(), reason.clone());
            }
            continue;
        }

        for (invoice, order) in sorted_invoices.into_iter().zip(sorted_orders) {
            candidates_by_invoice.insert(
                invoice.id.clone(),
                (
                    vec![MatchCandidate {
                        order_ids: vec![order.id.clone()],
                        orders: vec![order.summary()],
                        total_fen: order.paid_amount_fen,
                        shop_name: normalized_shop(order.shop_name.as_deref()),
                    }],
                    false,
                ),
            );
        }
    }
    conflicts
}

fn sorted_complete_dates<'a, T>(
    values: &'a [&'a T],
    date: impl Fn(&T) -> Option<&str>,
    id: impl Fn(&T) -> &str,
) -> Option<Vec<&'a T>> {
    let mut dated = values
        .iter()
        .map(|value| {
            date(value)
                .and_then(|date| date_part(Some(date)))
                .map(|date| (date, id(value), *value))
        })
        .collect::<Option<Vec<_>>>()?;
    dated.sort_by(|(left_date, left_id, _), (right_date, right_id, _)| {
        left_date
            .cmp(right_date)
            .then_with(|| left_id.cmp(right_id))
    });
    Some(dated.into_iter().map(|(_, _, value)| value).collect())
}

fn mark_repurchase_missing_date_conflict(
    conflicts: &mut HashMap<String, String>,
    invoices: &[&InvoiceRow],
) {
    let reason = "疑似回购冲突：日期缺失，无法按时间可靠配对".to_owned();
    for invoice in invoices {
        conflicts.insert(invoice.id.clone(), reason.clone());
    }
}

fn generate_candidates(
    target: i64,
    invoice_date: Option<&str>,
    orders: &[OrderRow],
    max_orders: usize,
    max_candidates: usize,
) -> (Vec<MatchCandidate>, bool) {
    let eligible = orders
        .iter()
        .filter(|value| date_allowed(value.ordered_at.as_deref(), invoice_date))
        .filter(|value| value.paid_amount_fen >= 0 && value.paid_amount_fen <= target)
        .cloned()
        .collect::<Vec<_>>();
    let mut index_sets = Vec::<Vec<usize>>::new();
    for (index, value) in eligible.iter().enumerate() {
        if value.paid_amount_fen == target {
            index_sets.push(vec![index]);
            if index_sets.len() > max_candidates {
                break;
            }
        }
    }
    if index_sets.len() <= max_candidates && max_orders >= 2 {
        let search_budget = max_candidates.saturating_mul(100).max(10_000);
        let mut visited = 0usize;
        let mut search_exhausted = false;
        let mut groups = HashMap::<String, Vec<usize>>::new();
        for (index, value) in eligible.iter().enumerate() {
            if let Some(shop) = normalized_shop(value.shop_name.as_deref()) {
                groups.entry(shop).or_default().push(index);
            }
        }
        let mut names = groups.into_keys().collect::<Vec<_>>();
        names.sort();
        for name in names {
            let indexes = eligible
                .iter()
                .enumerate()
                .filter_map(|(index, value)| {
                    (normalized_shop(value.shop_name.as_deref()).as_deref() == Some(&name))
                        .then_some(index)
                })
                .collect::<Vec<_>>();
            collect_combinations(
                target,
                &eligible,
                &indexes,
                0,
                max_orders,
                &mut Vec::new(),
                0,
                max_candidates + 1,
                &mut index_sets,
                &mut visited,
                search_budget,
                &mut search_exhausted,
            );
            if index_sets.len() > max_candidates || search_exhausted {
                break;
            }
        }
        if search_exhausted && index_sets.len() <= max_candidates {
            index_sets.push(Vec::new());
        }
    }
    index_sets.sort_by(|left, right| {
        let left_ids = left
            .iter()
            .map(|index| &eligible[*index].id)
            .collect::<Vec<_>>();
        let right_ids = right
            .iter()
            .map(|index| &eligible[*index].id)
            .collect::<Vec<_>>();
        left_ids.cmp(&right_ids)
    });
    index_sets.dedup();
    let overflow = index_sets.len() > max_candidates || index_sets.iter().any(Vec::is_empty);
    index_sets.retain(|indexes| !indexes.is_empty());
    index_sets.truncate(max_candidates);
    let candidates = index_sets
        .into_iter()
        .map(|indexes| {
            let selected = indexes
                .iter()
                .map(|index| eligible[*index].summary())
                .collect::<Vec<_>>();
            MatchCandidate {
                order_ids: selected.iter().map(|value| value.id.clone()).collect(),
                total_fen: selected.iter().map(|value| value.paid_amount_fen).sum(),
                shop_name: common_shop(&selected),
                orders: selected,
            }
        })
        .collect();
    (candidates, overflow)
}

#[allow(clippy::too_many_arguments)]
fn collect_combinations(
    target: i64,
    orders: &[OrderRow],
    indexes: &[usize],
    start: usize,
    max_orders: usize,
    selected: &mut Vec<usize>,
    sum: i64,
    limit: usize,
    result: &mut Vec<Vec<usize>>,
    visited: &mut usize,
    search_budget: usize,
    search_exhausted: &mut bool,
) {
    if result.len() >= limit || selected.len() >= max_orders || *search_exhausted {
        return;
    }
    for position in start..indexes.len() {
        *visited += 1;
        if *visited > search_budget {
            *search_exhausted = true;
            return;
        }
        let index = indexes[position];
        let next_sum = sum + orders[index].paid_amount_fen;
        if next_sum > target {
            continue;
        }
        selected.push(index);
        if selected.len() >= 2 && next_sum == target {
            result.push(selected.clone());
        } else if next_sum < target {
            collect_combinations(
                target,
                orders,
                indexes,
                position + 1,
                max_orders,
                selected,
                next_sum,
                limit,
                result,
                visited,
                search_budget,
                search_exhausted,
            );
        }
        selected.pop();
        if result.len() >= limit {
            return;
        }
    }
}

fn match_differences(
    invoice_total: i64,
    invoice_date: Option<&str>,
    orders: &[&order::Model],
) -> Vec<String> {
    let mut differences = Vec::new();
    let sum = orders
        .iter()
        .filter_map(|value| value.paid_amount_fen)
        .sum::<i64>();
    if sum != invoice_total {
        differences.push(format!("订单合计与发票金额相差 {} 分", sum - invoice_total));
    }
    if orders
        .iter()
        .any(|value| !date_allowed(value.ordered_at.as_deref(), invoice_date))
    {
        differences.push("存在晚于开票日期的订单".into());
    }
    if orders.len() > 1 {
        let shops = orders
            .iter()
            .map(|value| normalized_shop(value.shop_name.as_deref()))
            .collect::<Vec<_>>();
        if shops.iter().any(Option::is_none)
            || shops
                .first()
                .is_some_and(|first| shops.iter().any(|value| value != first))
        {
            differences.push("多张订单的店铺名称不一致或缺失".into());
        }
    }
    differences
}

fn date_allowed(order_date: Option<&str>, invoice_date: Option<&str>) -> bool {
    match (date_part(order_date), date_part(invoice_date)) {
        (Some(order), Some(invoice)) => order <= invoice,
        _ => true,
    }
}

fn date_part(value: Option<&str>) -> Option<&str> {
    let value = value?.trim();
    (!value.is_empty()).then(|| value.split([' ', 'T']).next().unwrap_or(value))
}

fn normalized_shop(value: Option<&str>) -> Option<String> {
    let normalized = value?
        .replace('\u{3000}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!normalized.is_empty()).then_some(normalized)
}

fn common_shop(orders: &[MatchOrderSummary]) -> Option<String> {
    let first = normalized_shop(orders.first()?.shop_name.as_deref())?;
    orders
        .iter()
        .all(|value| normalized_shop(value.shop_name.as_deref()).as_deref() == Some(&first))
        .then_some(first)
}

#[tauri::command]
pub async fn get_matching_page(
    state: State<'_, AppState>,
    query: MatchingQuery,
) -> CommandResult<MatchingPage> {
    let settings = state.settings.private().await;
    state
        .database
        .matching_page(
            query,
            usize::from(settings.match_max_orders),
            usize::from(settings.match_max_candidates),
        )
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn list_matchable_orders(
    state: State<'_, AppState>,
    query: MatchableOrdersQuery,
) -> CommandResult<MatchableOrdersPage> {
    state
        .database
        .list_matchable_orders(query)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn confirm_invoice_match(
    state: State<'_, AppState>,
    request: ConfirmInvoiceMatchRequest,
) -> CommandResult<ConfirmInvoiceMatchResult> {
    let settings = state.settings.private().await;
    validate_automatic_suggestion(&state, &request, &settings, false).await?;
    let result = state
        .database
        .confirm_invoice_match_with_summary_status(
            request,
            match settings.summary_generation_mode {
                crate::models::SummaryGenerationMode::Automatic => "pending",
                crate::models::SummaryGenerationMode::Manual => "not_generated",
            },
        )
        .await
        .map_err(crate::error::CommandError::from)?;
    if result.confirmed {
        state.match_summary_queue.wake();
    }
    Ok(result)
}

#[tauri::command]
pub async fn confirm_invoice_matches(
    state: State<'_, AppState>,
    request: ConfirmInvoiceMatchesRequest,
) -> CommandResult<Vec<ConfirmInvoiceMatchResult>> {
    if request.matches.is_empty() || request.matches.len() > 100 {
        return Err(AppError::InvalidInput("批量请选择 1 到 100 个匹配建议".into()).into());
    }
    let settings = state.settings.private().await;
    for item in &request.matches {
        validate_automatic_suggestion(&state, item, &settings, true).await?;
    }
    let results = state
        .database
        .confirm_invoice_matches_with_summary_status(
            request.matches,
            match settings.summary_generation_mode {
                crate::models::SummaryGenerationMode::Automatic => "pending",
                crate::models::SummaryGenerationMode::Manual => "not_generated",
            },
        )
        .await
        .map_err(crate::error::CommandError::from)?;
    state.match_summary_queue.wake();
    Ok(results)
}

async fn validate_automatic_suggestion(
    state: &AppState,
    request: &ConfirmInvoiceMatchRequest,
    settings: &crate::models::PrivateAppSettings,
    require_suggested: bool,
) -> CommandResult<()> {
    if request.source != "automatic" {
        return Ok(());
    }
    let page = state
        .database
        .matching_page(
            MatchingQuery {
                page: Some(1),
                page_size: Some(1),
                search: Some(request.invoice_id.clone()),
                status: Some(
                    if require_suggested {
                        "suggested"
                    } else {
                        "all"
                    }
                    .into(),
                ),
                summary_status: None,
            },
            usize::from(settings.match_max_orders),
            usize::from(settings.match_max_candidates),
        )
        .await
        .map_err(crate::error::CommandError::from)?;
    let valid = page.items.first().is_some_and(|item| {
        item.id == request.invoice_id
            && (!require_suggested || item.status == "suggested")
            && matches!(item.status.as_str(), "suggested" | "conflict")
            && item
                .candidates
                .iter()
                .any(|candidate| candidate.order_ids == request.order_ids)
    });
    if !valid {
        return Err(AppError::InvalidInput("匹配建议已变化，请刷新后重试".into()).into());
    }
    Ok(())
}

#[tauri::command]
pub async fn unconfirm_invoice_match(
    state: State<'_, AppState>,
    request: IdRequest,
) -> CommandResult<()> {
    state
        .database
        .unconfirm_invoice_match(&request.id)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn retry_match_summary(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: IdRequest,
) -> CommandResult<()> {
    state
        .database
        .retry_match_summary(&request.id)
        .await
        .map_err(crate::error::CommandError::from)?;
    crate::match_summary_queue::emit_update(&app, &request.id, "pending");
    state.match_summary_queue.wake();
    Ok(())
}

#[tauri::command]
pub async fn list_match_summaries(
    state: State<'_, AppState>,
    query: MatchSummariesQuery,
) -> CommandResult<MatchSummariesPage> {
    state
        .database
        .clean_expired_match_summary_attempts()
        .await
        .map_err(crate::error::CommandError::from)?;
    state
        .database
        .list_match_summaries(query)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn list_match_summary_attempts(
    state: State<'_, AppState>,
    query: MatchSummaryAttemptsQuery,
) -> CommandResult<MatchSummaryAttemptsPage> {
    state
        .database
        .clean_expired_match_summary_attempts()
        .await
        .map_err(crate::error::CommandError::from)?;
    state
        .database
        .list_match_summary_attempts(query)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn enqueue_match_summary(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: IdRequest,
) -> CommandResult<()> {
    state
        .database
        .enqueue_match_summary(&request.id)
        .await
        .map_err(crate::error::CommandError::from)?;
    crate::match_summary_queue::emit_update(&app, &request.id, "pending");
    state.match_summary_queue.wake();
    Ok(())
}

#[tauri::command]
pub async fn enqueue_all_match_summaries(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<u64> {
    let affected = state
        .database
        .enqueue_all_match_summaries()
        .await
        .map_err(crate::error::CommandError::from)?;
    if affected > 0 {
        crate::match_summary_queue::emit_update(&app, "", "pending");
        state.match_summary_queue.wake();
    }
    Ok(affected)
}

#[tauri::command]
pub async fn update_match_summary(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: UpdateMatchSummaryRequest,
) -> CommandResult<()> {
    state
        .database
        .update_match_summary(&request.invoice_id, &request.display_name)
        .await
        .map_err(crate::error::CommandError::from)?;
    crate::match_summary_queue::emit_update(&app, &request.invoice_id, "succeeded");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        AssignInvoicesToProjectRequest, InvoiceSort, InvoicesQuery, OrderSort, OrdersQuery,
        SortDirection, UpdateOrderRequest,
    };
    use tempfile::TempDir;

    async fn test_state() -> (TempDir, AppState) {
        let directory = tempfile::tempdir().unwrap();
        let app_data_dir = directory.path().to_path_buf();
        let images_dir = app_data_dir.join("images");
        tokio::fs::create_dir_all(&images_dir).await.unwrap();
        let database = Database::in_memory().await.unwrap();
        let state = AppState::test(database, app_data_dir, images_dir).await;
        (directory, state)
    }

    fn order(id: &str, amount: i64, date: Option<&str>, shop: Option<&str>) -> OrderRow {
        OrderRow {
            id: id.into(),
            external_order_number: Some(id.into()),
            paid_amount_fen: amount,
            ordered_at: date.map(str::to_owned),
            shop_name: shop.map(str::to_owned),
            product_summary: String::new(),
        }
    }

    async fn seed_records(db: &Database, orders: &[(&str, i64)], invoices: &[(&str, i64)]) {
        db.execute_test_statement(
            "INSERT INTO import_batches (id, platform, status, created_at) VALUES ('order-batch', 'taobao', 'completed', '2026-08-12T00:00:00Z');
             INSERT INTO invoice_batches (id, status, created_at) VALUES ('invoice-batch', 'completed', '2026-08-12T00:00:00Z');
             INSERT INTO invoice_tasks (id, batch_id, original_file_name, source_sha256, media_type, status, retry_count, created_at) VALUES ('invoice-task', 'invoice-batch', 'invoice.pdf', 'invoice-hash', 'application/pdf', 'succeeded', 0, '2026-08-12T00:00:00Z');",
        )
        .await
        .unwrap();
        for (id, amount) in orders {
            db.execute_test_statement(&format!(
                "INSERT INTO import_tasks (id, batch_id, original_file_name, image_sha256, status, retry_count, created_at) VALUES ('{id}', 'order-batch', '{id}.png', 'hash-{id}', 'succeeded', 0, '2026-08-01T00:00:00Z');
                 INSERT INTO orders (id, platform, external_order_number, paid_amount_fen, ordered_at, shop_name, raw_ocr_json, created_at, updated_at) VALUES ('{id}', 'taobao', 'NO-{id}', {amount}, '2026-08-01 08:00', '同店', '{{}}', '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z');",
            ))
            .await
            .unwrap();
        }
        for (id, amount) in invoices {
            db.execute_test_statement(&format!(
                "INSERT INTO invoices (id, task_id, type, subtype, type_description, subtype_description, invoice_number, invoice_date, total_fen, currency, preview_path, raw_invoice_json, created_at, updated_at) VALUES ('{id}', 'invoice-task', 3, 'VatInvoice', '增值税发票', '增值税发票', 'NO-{id}', '2026-08-10', {amount}, 'CNY', '{id}.pdf', '{{}}', '2026-08-10T00:00:00Z', '2026-08-10T00:00:00Z');",
            ))
            .await
            .unwrap();
        }
    }

    #[test]
    fn generates_single_and_same_shop_multi_order_candidates() {
        let orders = vec![
            order("a", 100, Some("2026-08-01"), None),
            order("b", 40, Some("2026-08-02"), Some(" 店　铺 ")),
            order("c", 60, None, Some("店 铺")),
            order("d", 60, Some("2026-08-20"), Some("店铺")),
        ];
        let (candidates, overflow) = generate_candidates(100, Some("2026-08-10"), &orders, 10, 100);
        assert!(!overflow);
        assert!(candidates.iter().any(|value| value.order_ids == ["a"]));
        assert!(candidates.iter().any(|value| value.order_ids == ["b", "c"]));
        assert!(
            !candidates
                .iter()
                .any(|value| value.order_ids.contains(&"d".into()))
        );
    }

    #[test]
    fn missing_date_skips_date_comparison() {
        assert!(date_allowed(None, Some("2026-08-10")));
        assert!(date_allowed(Some("  "), Some("2026-08-10")));
        assert!(date_allowed(Some("2026-08-11"), None));
        assert!(!date_allowed(Some("2026-08-11 09:00"), Some("2026-08-10")));
    }

    #[test]
    fn stops_after_candidate_limit() {
        let orders = (0..8)
            .map(|index| order(&format!("o{index}"), 50, None, Some("同店")))
            .collect::<Vec<_>>();
        let (candidates, overflow) = generate_candidates(100, None, &orders, 10, 3);
        assert!(overflow);
        assert_eq!(candidates.len(), 3);
    }

    #[test]
    fn stops_when_no_solution_search_space_is_too_large() {
        let orders = (0..80)
            .map(|index| order(&format!("o{index}"), 6, None, Some("同店")))
            .collect::<Vec<_>>();
        let (candidates, overflow) = generate_candidates(101, None, &orders, 20, 10);
        assert!(overflow);
        assert!(candidates.is_empty());
    }

    #[tokio::test]
    async fn cross_invoice_order_use_is_a_conflict() {
        let db = Database::in_memory().await.unwrap();
        seed_records(
            &db,
            &[("order-a", 100)],
            &[("invoice-a", 100), ("invoice-b", 100)],
        )
        .await;

        let page = db
            .matching_page(
                MatchingQuery {
                    page: None,
                    page_size: None,
                    search: None,
                    status: None,
                    summary_status: None,
                },
                10,
                100,
            )
            .await
            .unwrap();

        assert_eq!(page.summary.conflict, 2);
        assert!(page.items.iter().all(|item| item.status == "conflict"));
        let projection = db.ledger_match_projection(10, 100).await.unwrap();
        assert!(
            projection
                .invoice_statuses
                .values()
                .all(|status| status == "unmatched")
        );
        assert!(projection.order_statuses.is_empty());
    }

    #[tokio::test]
    async fn ledger_projection_distinguishes_suggestions_and_confirmations() {
        let db = Database::in_memory().await.unwrap();
        seed_records(&db, &[("order-a", 100)], &[("invoice-a", 100)]).await;

        let suggested = db.ledger_match_projection(10, 100).await.unwrap();
        assert_eq!(suggested.invoice_statuses["invoice-a"], "matched");
        assert_eq!(suggested.order_statuses["order-a"], "matched");

        db.confirm_invoice_match(ConfirmInvoiceMatchRequest {
            invoice_id: "invoice-a".into(),
            order_ids: vec!["order-a".into()],
            source: "automatic".into(),
            accept_mismatches: false,
        })
        .await
        .unwrap();
        let confirmed = db.ledger_match_projection(10, 100).await.unwrap();
        assert_eq!(confirmed.invoice_statuses["invoice-a"], "confirmed");
        assert_eq!(confirmed.order_statuses["order-a"], "confirmed");
    }

    #[tokio::test]
    async fn summary_filter_uses_confirmed_matches_and_keeps_global_counts() {
        let db = Database::in_memory().await.unwrap();
        seed_records(
            &db,
            &[("order-a", 100), ("order-b", 200)],
            &[("invoice-a", 100), ("invoice-b", 200)],
        )
        .await;
        for (invoice_id, order_id) in [("invoice-a", "order-a"), ("invoice-b", "order-b")] {
            db.confirm_invoice_match(ConfirmInvoiceMatchRequest {
                invoice_id: invoice_id.into(),
                order_ids: vec![order_id.into()],
                source: "automatic".into(),
                accept_mismatches: false,
            })
            .await
            .unwrap();
        }
        db.execute_test_statement(
            "UPDATE invoice_matches SET summary_status='succeeded',display_name='商品甲' WHERE invoice_id='invoice-a';
             UPDATE invoice_matches SET summary_status='failed',summary_error='模型离线' WHERE invoice_id='invoice-b';",
        )
        .await
        .unwrap();

        let page = db
            .matching_page(
                MatchingQuery {
                    page: Some(1),
                    page_size: Some(1),
                    search: None,
                    status: Some("confirmed".into()),
                    summary_status: Some("failed".into()),
                },
                10,
                100,
            )
            .await
            .unwrap();

        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].id, "invoice-b");
        assert_eq!(page.summary.confirmed, 2);
        assert_eq!(page.summary_counts.total, 2);
        assert_eq!(page.summary_counts.succeeded, 1);
        assert_eq!(page.summary_counts.failed, 1);
    }

    #[tokio::test]
    async fn archived_project_members_leave_and_reenter_daily_workspaces() {
        let db = Database::in_memory().await.unwrap();
        seed_records(&db, &[("order-a", 100)], &[("invoice-a", 100)]).await;
        db.confirm_invoice_match(ConfirmInvoiceMatchRequest {
            invoice_id: "invoice-a".into(),
            order_ids: vec!["order-a".into()],
            source: "automatic".into(),
            accept_mismatches: false,
        })
        .await
        .unwrap();
        let app_data = tempfile::tempdir().unwrap();
        let project = db
            .create_project("归档项目".into(), app_data.path())
            .await
            .unwrap();
        db.assign_invoices_to_project(AssignInvoicesToProjectRequest {
            invoice_ids: vec!["invoice-a".into()],
            project_id: project.id.clone(),
            accept_reassignment: false,
        })
        .await
        .unwrap();

        let order_query = || OrdersQuery {
            page: None,
            page_size: None,
            search: None,
            duplicate_only: None,
            match_status: None,
            project_status: None,
            project_id: None,
            sort_by: Some(OrderSort::CreatedAt),
            sort_direction: Some(SortDirection::Desc),
        };
        let invoice_query = || InvoicesQuery {
            page: None,
            page_size: None,
            search: None,
            subtype: None,
            date_from: None,
            date_to: None,
            duplicate_only: None,
            match_status: None,
            project_status: None,
            project_id: None,
            sort_by: Some(InvoiceSort::CreatedAt),
            sort_direction: Some(SortDirection::Desc),
        };
        let orders = db
            .list_orders_with_matching(order_query(), 10, 100)
            .await
            .unwrap();
        let invoices = db
            .list_invoices_with_matching(invoice_query(), 10, 100)
            .await
            .unwrap();
        assert_eq!(orders.items[0].match_status, "confirmed");
        assert_eq!(orders.items[0].project_name.as_deref(), Some("归档项目"));
        assert_eq!(invoices.items[0].match_status, "confirmed");

        db.set_project_archived(&project.id, true).await.unwrap();
        assert_eq!(
            db.list_orders_with_matching(order_query(), 10, 100)
                .await
                .unwrap()
                .total,
            0
        );
        assert_eq!(
            db.list_invoices_with_matching(invoice_query(), 10, 100)
                .await
                .unwrap()
                .total,
            0
        );
        assert_eq!(
            db.matching_page(
                MatchingQuery {
                    page: None,
                    page_size: None,
                    search: None,
                    status: None,
                    summary_status: None,
                },
                10,
                100,
            )
            .await
            .unwrap()
            .total,
            0
        );

        db.set_project_archived(&project.id, false).await.unwrap();
        assert_eq!(
            db.list_orders_with_matching(order_query(), 10, 100)
                .await
                .unwrap()
                .total,
            1
        );
        assert_eq!(
            db.list_invoices_with_matching(invoice_query(), 10, 100)
                .await
                .unwrap()
                .total,
            1
        );
    }

    #[tokio::test]
    async fn chronologically_resolves_equal_repurchase_groups() {
        let db = Database::in_memory().await.unwrap();
        seed_records(
            &db,
            &[("order-early", 100), ("order-late", 100)],
            &[("invoice-early", 100), ("invoice-late", 100)],
        )
        .await;
        db.execute_test_statement(
            "UPDATE orders SET ordered_at='2026-08-02 18:00',shop_name=' 同　店 ' WHERE id='order-early';
             UPDATE orders SET ordered_at='2026-08-08 09:00',shop_name='同 店' WHERE id='order-late';
             INSERT INTO order_items (order_id,position,product_name) VALUES ('order-early',0,'商品甲'),('order-late',0,'完全不同的商品乙');
             UPDATE invoices SET invoice_date='2026-08-03',seller_name=' 销　售 方 ',invoice_number='UNIQUE-A' WHERE id='invoice-early';
             UPDATE invoices SET invoice_date='2026-08-10',seller_name='销 售 方',invoice_number='UNIQUE-B' WHERE id='invoice-late';",
        )
        .await
        .unwrap();

        let page = db
            .matching_page(
                MatchingQuery {
                    page: None,
                    page_size: None,
                    search: None,
                    status: None,
                    summary_status: None,
                },
                10,
                100,
            )
            .await
            .unwrap();

        assert_eq!(page.summary.suggested, 2);
        assert_eq!(page.summary.conflict, 0);
        let suggestions = page
            .items
            .iter()
            .map(|item| (item.id.as_str(), item.candidates[0].order_ids[0].as_str()))
            .collect::<HashMap<_, _>>();
        assert_eq!(suggestions["invoice-early"], "order-early");
        assert_eq!(suggestions["invoice-late"], "order-late");
    }

    #[tokio::test]
    async fn repeated_order_dates_resolve_deterministically() {
        let db = Database::in_memory().await.unwrap();
        seed_records(
            &db,
            &[("order-z", 100), ("order-a", 100)],
            &[("invoice-early", 100), ("invoice-late", 100)],
        )
        .await;
        db.execute_test_statement(
            "UPDATE orders SET ordered_at='2026-08-01',shop_name='同店';
             UPDATE invoices SET invoice_date=CASE id WHEN 'invoice-early' THEN '2026-08-02' ELSE '2026-08-03' END,seller_name='销售方';",
        )
        .await
        .unwrap();

        let mut runs = Vec::new();
        for _ in 0..2 {
            let page = db
                .matching_page(
                    MatchingQuery {
                        page: None,
                        page_size: None,
                        search: None,
                        status: None,
                        summary_status: None,
                    },
                    10,
                    100,
                )
                .await
                .unwrap();
            assert_eq!(page.summary.suggested, 2);
            assert_eq!(page.summary.conflict, 0);
            runs.push(
                page.items
                    .into_iter()
                    .map(|item| (item.id, item.candidates[0].order_ids[0].clone()))
                    .collect::<HashMap<_, _>>(),
            );
        }

        assert_eq!(runs[0], runs[1]);
        assert_eq!(runs[0]["invoice-early"], "order-a");
        assert_eq!(runs[0]["invoice-late"], "order-z");
    }

    #[tokio::test]
    async fn repeated_invoice_dates_resolve_by_stable_id() {
        let db = Database::in_memory().await.unwrap();
        seed_records(
            &db,
            &[("order-early", 100), ("order-late", 100)],
            &[("invoice-z", 100), ("invoice-a", 100)],
        )
        .await;
        db.execute_test_statement(
            "UPDATE orders SET ordered_at=CASE id WHEN 'order-early' THEN '2026-08-01' ELSE '2026-08-02' END,shop_name='同店';
             UPDATE invoices SET invoice_date='2026-08-03',seller_name='销售方';",
        )
        .await
        .unwrap();

        let page = db
            .matching_page(
                MatchingQuery {
                    page: None,
                    page_size: None,
                    search: None,
                    status: None,
                    summary_status: None,
                },
                10,
                100,
            )
            .await
            .unwrap();

        assert_eq!(page.summary.suggested, 2);
        assert_eq!(page.summary.conflict, 0);
        let suggestions = page
            .items
            .iter()
            .map(|item| (item.id.as_str(), item.candidates[0].order_ids[0].as_str()))
            .collect::<HashMap<_, _>>();
        assert_eq!(suggestions["invoice-a"], "order-early");
        assert_eq!(suggestions["invoice-z"], "order-late");
    }

    #[tokio::test]
    async fn repeated_date_repurchase_suggestions_confirm_atomically() {
        let db = Database::in_memory().await.unwrap();
        seed_records(
            &db,
            &[("order-early", 100), ("order-late", 100)],
            &[("invoice-early", 100), ("invoice-late", 100)],
        )
        .await;
        db.execute_test_statement(
            "UPDATE orders SET ordered_at='2026-08-01';
             UPDATE invoices SET invoice_date='2026-08-02',seller_name='销售方';",
        )
        .await
        .unwrap();
        let page = db
            .matching_page(
                MatchingQuery {
                    page: None,
                    page_size: None,
                    search: None,
                    status: Some("suggested".into()),
                    summary_status: None,
                },
                10,
                100,
            )
            .await
            .unwrap();
        let requests = page
            .items
            .into_iter()
            .map(|item| ConfirmInvoiceMatchRequest {
                invoice_id: item.id,
                order_ids: item.candidates[0].order_ids.clone(),
                source: "automatic".into(),
                accept_mismatches: false,
            })
            .collect::<Vec<_>>();

        let results = db.confirm_invoice_matches(requests).await.unwrap();

        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|result| result.confirmed));
        assert_eq!(
            invoice_match::Entity::find()
                .all(&db.connection)
                .await
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            invoice_match_order::Entity::find()
                .all(&db.connection)
                .await
                .unwrap()
                .len(),
            2
        );
    }

    #[tokio::test]
    async fn automatic_revalidation_accepts_repeated_date_repurchase_suggestions() {
        let (_directory, state) = test_state().await;
        seed_records(
            &state.database,
            &[("order-early", 100), ("order-late", 100)],
            &[("invoice-early", 100), ("invoice-late", 100)],
        )
        .await;
        state
            .database
            .execute_test_statement(
                "UPDATE orders SET ordered_at='2026-08-01';
                 UPDATE invoices SET invoice_date='2026-08-02',seller_name='销售方';",
            )
            .await
            .unwrap();
        let settings = state.settings.private().await;

        for (invoice_id, order_id) in [
            ("invoice-early", "order-early"),
            ("invoice-late", "order-late"),
        ] {
            validate_automatic_suggestion(
                &state,
                &ConfirmInvoiceMatchRequest {
                    invoice_id: invoice_id.into(),
                    order_ids: vec![order_id.into()],
                    source: "automatic".into(),
                    accept_mismatches: false,
                },
                &settings,
                true,
            )
            .await
            .unwrap();
        }
    }

    #[tokio::test]
    async fn unequal_repurchase_groups_have_a_dedicated_conflict_reason() {
        let db = Database::in_memory().await.unwrap();
        seed_records(
            &db,
            &[("order-a", 100), ("order-b", 100)],
            &[("invoice-a", 100), ("invoice-b", 100), ("invoice-c", 100)],
        )
        .await;
        db.execute_test_statement(
            "UPDATE orders SET ordered_at=CASE id WHEN 'order-a' THEN '2026-08-01' ELSE '2026-08-02' END;
             UPDATE invoices SET invoice_date=CASE id WHEN 'invoice-a' THEN '2026-08-03' WHEN 'invoice-b' THEN '2026-08-04' ELSE '2026-08-05' END,seller_name='销售方';",
        )
        .await
        .unwrap();

        let page = db
            .matching_page(
                MatchingQuery {
                    page: None,
                    page_size: None,
                    search: None,
                    status: None,
                    summary_status: None,
                },
                10,
                100,
            )
            .await
            .unwrap();

        assert_eq!(page.summary.conflict, 3);
        assert!(page.items.iter().all(|item| {
            item.reason.as_deref() == Some("疑似回购冲突：订单 2 条，发票 3 张，数量不一致")
        }));
    }

    #[tokio::test]
    async fn missing_repurchase_dates_remain_conflicts() {
        let db = Database::in_memory().await.unwrap();
        seed_records(
            &db,
            &[("order-a", 100), ("order-b", 100)],
            &[("invoice-a", 100), ("invoice-b", 100)],
        )
        .await;
        db.execute_test_statement(
            "UPDATE orders SET ordered_at=CASE id WHEN 'order-a' THEN NULL ELSE '2026-08-01' END;
             UPDATE invoices SET seller_name='销售方';",
        )
        .await
        .unwrap();

        let page = db
            .matching_page(
                MatchingQuery {
                    page: None,
                    page_size: None,
                    search: None,
                    status: None,
                    summary_status: None,
                },
                10,
                100,
            )
            .await
            .unwrap();

        assert_eq!(page.summary.conflict, 2);
        assert!(page.items.iter().all(|item| {
            item.reason.as_deref() == Some("疑似回购冲突：日期缺失，无法按时间可靠配对")
        }));
    }

    #[tokio::test]
    async fn missing_repurchase_invoice_date_remains_a_conflict() {
        let db = Database::in_memory().await.unwrap();
        seed_records(
            &db,
            &[("order-a", 100), ("order-b", 100)],
            &[("invoice-a", 100), ("invoice-b", 100)],
        )
        .await;
        db.execute_test_statement(
            "UPDATE orders SET ordered_at='2026-08-01';
             UPDATE invoices SET invoice_date=CASE id WHEN 'invoice-a' THEN NULL ELSE '2026-08-02' END,seller_name='销售方';",
        )
        .await
        .unwrap();

        let page = db
            .matching_page(
                MatchingQuery {
                    page: None,
                    page_size: None,
                    search: None,
                    status: None,
                    summary_status: None,
                },
                10,
                100,
            )
            .await
            .unwrap();

        assert_eq!(page.summary.conflict, 2);
        assert!(page.items.iter().all(|item| {
            item.reason.as_deref() == Some("疑似回购冲突：日期缺失，无法按时间可靠配对")
        }));
    }

    #[tokio::test]
    async fn late_chronological_repurchase_pair_remains_a_conflict() {
        let db = Database::in_memory().await.unwrap();
        seed_records(
            &db,
            &[("order-early", 100), ("order-late", 100)],
            &[("invoice-early", 100), ("invoice-late", 100)],
        )
        .await;
        db.execute_test_statement(
            "UPDATE orders SET ordered_at=CASE id WHEN 'order-early' THEN '2026-08-01' ELSE '2026-08-10' END;
             UPDATE invoices SET invoice_date=CASE id WHEN 'invoice-early' THEN '2026-08-02' ELSE '2026-08-03' END,seller_name='销售方';",
        )
        .await
        .unwrap();

        let page = db
            .matching_page(
                MatchingQuery {
                    page: None,
                    page_size: None,
                    search: None,
                    status: None,
                    summary_status: None,
                },
                10,
                100,
            )
            .await
            .unwrap();

        assert_eq!(page.summary.conflict, 2);
        assert!(page.items.iter().all(|item| {
            item.reason.as_deref() == Some("疑似回购冲突：按时间配对后存在订单晚于开票日期")
        }));
    }

    #[tokio::test]
    async fn different_invoice_business_fields_do_not_form_a_repurchase_group() {
        let db = Database::in_memory().await.unwrap();
        seed_records(
            &db,
            &[("order-a", 100), ("order-b", 100)],
            &[("invoice-a", 100), ("invoice-b", 100)],
        )
        .await;
        db.execute_test_statement(
            "UPDATE orders SET ordered_at=CASE id WHEN 'order-a' THEN '2026-08-01' ELSE '2026-08-02' END;
             UPDATE invoices SET invoice_date=CASE id WHEN 'invoice-a' THEN '2026-08-03' ELSE '2026-08-04' END;
             UPDATE invoices SET seller_name='销售方甲' WHERE id='invoice-a';
             UPDATE invoices SET seller_name='销售方乙' WHERE id='invoice-b';",
        )
        .await
        .unwrap();

        let page = db
            .matching_page(
                MatchingQuery {
                    page: None,
                    page_size: None,
                    search: None,
                    status: None,
                    summary_status: None,
                },
                10,
                100,
            )
            .await
            .unwrap();

        assert_eq!(page.summary.conflict, 2);
        assert!(page.items.iter().all(|item| {
            item.reason.as_deref() == Some("存在多个订单组合，需要手动选择")
        }));
    }

    #[tokio::test]
    async fn different_order_shops_do_not_form_a_repurchase_group() {
        let db = Database::in_memory().await.unwrap();
        seed_records(
            &db,
            &[("order-a", 100), ("order-b", 100)],
            &[("invoice-a", 100), ("invoice-b", 100)],
        )
        .await;
        db.execute_test_statement(
            "UPDATE orders SET ordered_at=CASE id WHEN 'order-a' THEN '2026-08-01' ELSE '2026-08-02' END;
             UPDATE orders SET shop_name='店铺甲' WHERE id='order-a';
             UPDATE orders SET shop_name='店铺乙' WHERE id='order-b';
             UPDATE invoices SET invoice_date=CASE id WHEN 'invoice-a' THEN '2026-08-03' ELSE '2026-08-04' END,seller_name='销售方';",
        )
        .await
        .unwrap();

        let page = db
            .matching_page(
                MatchingQuery {
                    page: None,
                    page_size: None,
                    search: None,
                    status: None,
                    summary_status: None,
                },
                10,
                100,
            )
            .await
            .unwrap();

        assert_eq!(page.summary.conflict, 2);
        assert!(page.items.iter().all(|item| {
            item.reason.as_deref() == Some("存在多个订单组合，需要手动选择")
        }));
    }

    #[tokio::test]
    async fn different_invoice_subtypes_do_not_form_a_repurchase_group() {
        let db = Database::in_memory().await.unwrap();
        seed_records(
            &db,
            &[("order-a", 100), ("order-b", 100)],
            &[("invoice-a", 100), ("invoice-b", 100)],
        )
        .await;
        db.execute_test_statement(
            "UPDATE orders SET ordered_at=CASE id WHEN 'order-a' THEN '2026-08-01' ELSE '2026-08-02' END;
             UPDATE invoices SET invoice_date=CASE id WHEN 'invoice-a' THEN '2026-08-03' ELSE '2026-08-04' END,seller_name='销售方';
             UPDATE invoices SET subtype='VatInvoice' WHERE id='invoice-a';
             UPDATE invoices SET subtype='VatSpecialInvoice' WHERE id='invoice-b';",
        )
        .await
        .unwrap();

        let page = db
            .matching_page(
                MatchingQuery {
                    page: None,
                    page_size: None,
                    search: None,
                    status: None,
                    summary_status: None,
                },
                10,
                100,
            )
            .await
            .unwrap();

        assert_eq!(page.summary.conflict, 2);
        assert!(page.items.iter().all(|item| {
            item.reason.as_deref() == Some("存在多个订单组合，需要手动选择")
        }));
    }

    #[tokio::test]
    async fn custom_difference_requires_explicit_acceptance() {
        let db = Database::in_memory().await.unwrap();
        seed_records(&db, &[("order-a", 90)], &[("invoice-a", 100)]).await;
        let request = ConfirmInvoiceMatchRequest {
            invoice_id: "invoice-a".into(),
            order_ids: vec!["order-a".into()],
            source: "custom".into(),
            accept_mismatches: false,
        };

        let preview = db.confirm_invoice_match(request.clone()).await.unwrap();
        assert!(!preview.confirmed);
        assert_eq!(preview.differences.len(), 1);
        let confirmed = db
            .confirm_invoice_match(ConfirmInvoiceMatchRequest {
                accept_mismatches: true,
                ..request
            })
            .await
            .unwrap();
        assert!(confirmed.confirmed);
    }

    #[tokio::test]
    async fn custom_order_list_disables_missing_and_invalid_amounts() {
        let db = Database::in_memory().await.unwrap();
        seed_records(&db, &[("negative", -1)], &[]).await;
        db.execute_test_statement(
            "INSERT INTO import_tasks (id, batch_id, original_file_name, image_sha256, status, retry_count, created_at) VALUES ('missing', 'order-batch', 'missing.png', 'hash-missing', 'succeeded', 0, '2026-08-01T00:00:00Z');
             INSERT INTO orders (id, platform, external_order_number, paid_amount_fen, raw_ocr_json, created_at, updated_at) VALUES ('missing', 'taobao', 'NO-missing', NULL, '{}', '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z');",
        )
        .await
        .unwrap();

        let page = db
            .list_matchable_orders(MatchableOrdersQuery {
                page: None,
                page_size: None,
                search: None,
                sort_by: None,
                sort_direction: None,
            })
            .await
            .unwrap();
        let reasons = page
            .items
            .into_iter()
            .map(|item| (item.id, item.disabled_reason))
            .collect::<HashMap<_, _>>();
        assert_eq!(reasons["negative"].as_deref(), Some("订单金额无效"));
        assert_eq!(reasons["missing"].as_deref(), Some("订单金额缺失"));
    }

    #[tokio::test]
    async fn custom_order_list_excludes_confirmed_orders_before_paging() {
        let db = Database::in_memory().await.unwrap();
        seed_records(
            &db,
            &[("occupied", 100), ("available", 100)],
            &[("invoice-a", 100)],
        )
        .await;
        db.confirm_invoice_match(ConfirmInvoiceMatchRequest {
            invoice_id: "invoice-a".into(),
            order_ids: vec!["occupied".into()],
            source: "custom".into(),
            accept_mismatches: false,
        })
        .await
        .unwrap();

        let page = db
            .list_matchable_orders(MatchableOrdersQuery {
                page: Some(1),
                page_size: Some(1),
                search: None,
                sort_by: None,
                sort_direction: None,
            })
            .await
            .unwrap();

        assert_eq!(page.total, 1);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, "available");
        assert!(page.items[0].matched_invoice_id.is_none());
    }

    #[tokio::test]
    async fn confirmed_match_blocks_delete_and_key_edit_releases_it() {
        let db = Database::in_memory().await.unwrap();
        seed_records(&db, &[("order-a", 100)], &[("invoice-a", 100)]).await;
        db.confirm_invoice_match(ConfirmInvoiceMatchRequest {
            invoice_id: "invoice-a".into(),
            order_ids: vec!["order-a".into()],
            source: "automatic".into(),
            accept_mismatches: false,
        })
        .await
        .unwrap();

        assert!(matches!(
            db.delete_order_record("order-a").await,
            Err(AppError::MatchingLocked(_))
        ));
        assert!(matches!(
            db.invoice_paths_for_delete("invoice-a").await,
            Err(AppError::MatchingLocked(_))
        ));
        let updated = db
            .update_order(UpdateOrderRequest {
                id: "order-a".into(),
                external_order_number: Some("NO-order-a".into()),
                product_names: Vec::new(),
                paid_amount_fen: Some(99),
                ordered_at: Some("2026-08-01 08:00".into()),
                shop_name: Some("同店".into()),
            })
            .await
            .unwrap();
        assert!(!updated.is_matched);
        assert!(db.delete_order_record("order-a").await.is_ok());
    }

    #[tokio::test]
    async fn unconfirm_releases_order_for_another_invoice() {
        let db = Database::in_memory().await.unwrap();
        seed_records(
            &db,
            &[("order-a", 100)],
            &[("invoice-a", 100), ("invoice-b", 100)],
        )
        .await;
        db.confirm_invoice_match(ConfirmInvoiceMatchRequest {
            invoice_id: "invoice-a".into(),
            order_ids: vec!["order-a".into()],
            source: "custom".into(),
            accept_mismatches: false,
        })
        .await
        .unwrap();
        db.unconfirm_invoice_match("invoice-a").await.unwrap();
        let next = db
            .confirm_invoice_match(ConfirmInvoiceMatchRequest {
                invoice_id: "invoice-b".into(),
                order_ids: vec!["order-a".into()],
                source: "custom".into(),
                accept_mismatches: false,
            })
            .await
            .unwrap();
        assert!(next.confirmed);
    }

    #[tokio::test]
    async fn batch_confirmation_rolls_back_every_item_on_failure() {
        let db = Database::in_memory().await.unwrap();
        seed_records(
            &db,
            &[("order-a", 100), ("order-b", 200)],
            &[("invoice-a", 100), ("invoice-b", 200)],
        )
        .await;

        let result = db
            .confirm_invoice_matches(vec![
                ConfirmInvoiceMatchRequest {
                    invoice_id: "invoice-a".into(),
                    order_ids: vec!["order-a".into()],
                    source: "automatic".into(),
                    accept_mismatches: false,
                },
                ConfirmInvoiceMatchRequest {
                    invoice_id: "invoice-b".into(),
                    order_ids: vec!["missing-order".into()],
                    source: "automatic".into(),
                    accept_mismatches: false,
                },
            ])
            .await;

        assert!(result.is_err());
        assert!(
            invoice_match::Entity::find()
                .all(&db.connection)
                .await
                .unwrap()
                .is_empty()
        );
    }
}
