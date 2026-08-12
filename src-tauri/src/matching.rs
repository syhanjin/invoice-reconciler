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
    MatchCandidate, MatchOrderSummary, MatchableOrderItem, MatchableOrdersPage,
    MatchableOrdersQuery, MatchingInvoiceItem, MatchingPage, MatchingQuery, MatchingSummary,
    OrderSort, SortDirection,
};

#[derive(Clone, Debug, FromQueryResult)]
struct InvoiceRow {
    id: String,
    invoice_number: Option<String>,
    subtype_description: String,
    invoice_date: Option<String>,
    seller_name: Option<String>,
    total_fen: Option<i64>,
    currency: String,
    confirmed_source: Option<String>,
    confirmed_at: Option<String>,
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
    pub async fn matching_page(
        &self,
        query: MatchingQuery,
        max_orders: usize,
        max_candidates: usize,
    ) -> AppResult<MatchingPage> {
        let invoices = InvoiceRow::find_by_statement(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT i.id,i.invoice_number,i.subtype_description,i.invoice_date,i.seller_name,\
             i.total_fen,i.currency,m.source confirmed_source,m.confirmed_at \
             FROM invoices i LEFT JOIN invoice_matches m ON m.invoice_id=i.id \
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
        let mut summary = MatchingSummary::default();
        for value in invoices {
            let (status, reason, candidates) = if value.confirmed_source.is_some() {
                (
                    "confirmed".to_owned(),
                    None,
                    confirmed_orders
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
            summary.total += 1;
            match status.as_str() {
                "suggested" => summary.suggested += 1,
                "conflict" => summary.conflict += 1,
                "unmatched" => summary.unmatched += 1,
                "ineligible" => summary.ineligible += 1,
                "confirmed" => summary.confirmed += 1,
                _ => {}
            }
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
            });
        }

        let search = query.search.unwrap_or_default().trim().to_lowercase();
        let status_filter = query.status.unwrap_or_default();
        items.retain(|item| {
            (status_filter.is_empty() || status_filter == "all" || item.status == status_filter)
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
        let mut filter = String::new();
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
                 mo.invoice_id matched_invoice_id,CASE WHEN mo.order_id IS NOT NULL THEN '已被其他发票占用' WHEN o.paid_amount_fen IS NULL THEN '订单金额缺失' WHEN o.paid_amount_fen < 0 THEN '订单金额无效' ELSE NULL END disabled_reason \
                 FROM orders o LEFT JOIN invoice_match_orders mo ON mo.order_id=o.id WHERE 1=1{filter} \
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

    pub async fn confirm_invoice_match(
        &self,
        request: ConfirmInvoiceMatchRequest,
    ) -> AppResult<ConfirmInvoiceMatchResult> {
        let transaction = self.connection.begin().await?;
        let result = self.confirm_invoice_match_in(&transaction, request).await?;
        if result.confirmed {
            transaction.commit().await?;
        } else {
            transaction.rollback().await?;
        }
        Ok(result)
    }

    pub async fn confirm_invoice_matches(
        &self,
        requests: Vec<ConfirmInvoiceMatchRequest>,
    ) -> AppResult<Vec<ConfirmInvoiceMatchResult>> {
        let transaction = self.connection.begin().await?;
        let mut results = Vec::with_capacity(requests.len());
        for request in requests {
            let result = self.confirm_invoice_match_in(&transaction, request).await?;
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
    ) -> AppResult<ConfirmInvoiceMatchResult> {
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

async fn confirmed_orders(
    connection: &sea_orm::DatabaseConnection,
) -> AppResult<HashMap<String, MatchCandidate>> {
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
    let rows = ConfirmedOrderRow::find_by_statement(Statement::from_string(
        DbBackend::Sqlite,
        "SELECT mo.invoice_id,o.id,o.external_order_number,o.paid_amount_fen,o.ordered_at,o.shop_name,\
         COALESCE((SELECT group_concat(product_name,' · ') FROM (SELECT product_name FROM order_items WHERE order_id=o.id ORDER BY position LIMIT 2)),'') product_summary \
         FROM invoice_match_orders mo JOIN orders o ON o.id=mo.order_id ORDER BY mo.invoice_id,mo.position",
    ))
    .all(connection)
    .await?;
    let mut result = HashMap::<String, MatchCandidate>::new();
    for row in rows {
        let candidate = result
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
    for candidate in result.values_mut() {
        candidate.shop_name = common_shop(&candidate.orders);
    }
    Ok(result)
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
    state
        .database
        .confirm_invoice_match(request)
        .await
        .map_err(Into::into)
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
    state
        .database
        .confirm_invoice_matches(request.matches)
        .await
        .map_err(Into::into)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::UpdateOrderRequest;

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
                },
                10,
                100,
            )
            .await
            .unwrap();

        assert_eq!(page.summary.conflict, 2);
        assert!(page.items.iter().all(|item| item.status == "conflict"));
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
