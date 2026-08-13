use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, FromQueryResult,
    QueryFilter, QueryOrder, Set, Statement, TransactionTrait,
};
use sha2::{Digest, Sha256};

use crate::db::{Database, now};
use crate::entities::{
    invoice, invoice_line_item, invoice_match, invoice_match_order, match_summary_attempt, order,
    order_item,
};
use crate::error::{AppError, AppResult};
use crate::models::{
    ClaimedMatchSummary, MatchSummariesPage, MatchSummariesQuery, MatchSummaryAttemptItem,
    MatchSummaryAttemptsPage, MatchSummaryAttemptsQuery, MatchSummaryCounts, MatchSummaryInput,
    MatchSummaryInvoiceItem, MatchSummaryListItem, MatchSummaryOrder,
};

#[derive(FromQueryResult)]
struct CountRow {
    count: i64,
}

#[derive(FromQueryResult)]
struct SummaryStatusCountRow {
    summary_status: String,
    count: i64,
}

#[derive(FromQueryResult)]
struct MatchSummaryAttemptRow {
    id: String,
    invoice_id: String,
    invoice_number: Option<String>,
    input_json: String,
    trigger: String,
    provider: Option<String>,
    model: Option<String>,
    status: String,
    result_summary: Option<String>,
    error_message: Option<String>,
    started_at: String,
    completed_at: Option<String>,
}

const MATCH_SUMMARY_SELECT: &str = "SELECT m.invoice_id,i.invoice_number,i.seller_name,p.name project_name,\
 COALESCE(NULLIF(trim(m.display_name),''),\
   NULLIF((SELECT group_concat(product_name,'、') FROM (SELECT oi.product_name FROM invoice_match_orders mo JOIN order_items oi ON oi.order_id=mo.order_id WHERE mo.invoice_id=m.invoice_id AND trim(oi.product_name)<>'' ORDER BY mo.position,oi.position)),''),\
   NULLIF((SELECT group_concat(label,'、') FROM (SELECT COALESCE(NULLIF(o.external_order_number,''),o.id) label FROM invoice_match_orders mo JOIN orders o ON o.id=mo.order_id WHERE mo.invoice_id=m.invoice_id ORDER BY mo.position)),''),\
   '订单组合') display_name,\
 m.summary_status,m.summary_source,m.summary_error,m.summary_provider,m.summary_model,m.summary_generated_at,m.confirmed_at \
 FROM invoice_matches m JOIN invoices i ON i.id=m.invoice_id \
 LEFT JOIN project_invoices pi ON pi.invoice_id=i.id LEFT JOIN projects p ON p.id=pi.project_id";

pub(crate) fn display_name(
    generated: Option<&str>,
    product_names: &[String],
    order_labels: &[String],
) -> String {
    generated
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            let products = product_names
                .iter()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>()
                .join("、");
            (!products.is_empty()).then_some(products)
        })
        .unwrap_or_else(|| {
            let labels = order_labels.join("、");
            if labels.is_empty() {
                "订单组合".into()
            } else {
                labels
            }
        })
}

impl Database {
    pub async fn list_match_summaries(
        &self,
        query: MatchSummariesQuery,
    ) -> AppResult<MatchSummariesPage> {
        let page = query.page.unwrap_or(1).max(1);
        let page_size = query.page_size.unwrap_or(20).clamp(1, 100);
        let status = normalized_summary_status(query.status.as_deref())?;
        let mut counts = MatchSummaryCounts::default();
        for row in SummaryStatusCountRow::find_by_statement(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT summary_status,COUNT(*) count FROM invoice_matches GROUP BY summary_status",
        ))
        .all(&self.connection)
        .await?
        {
            counts.add_count(&row.summary_status, row.count);
        }

        let mut filter = String::new();
        let mut values = Vec::<sea_orm::Value>::new();
        if let Some(status) = status {
            filter.push_str(" AND summary_status=?");
            values.push(status.into());
        }
        if let Some(search) = query
            .search
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
        {
            filter.push_str(" AND (invoice_id LIKE ? OR invoice_number LIKE ? OR seller_name LIKE ? OR project_name LIKE ? OR display_name LIKE ?)");
            let pattern = format!("%{search}%");
            for _ in 0..5 {
                values.push(pattern.clone().into());
            }
        }
        let total = CountRow::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            format!("SELECT COUNT(*) count FROM ({MATCH_SUMMARY_SELECT}) WHERE 1=1{filter}"),
            values.clone(),
        ))
        .one(&self.connection)
        .await?
        .map_or(0, |row| row.count);
        let mut page_values = values;
        page_values.push(i64::from(page_size).into());
        page_values.push((i64::from(page - 1) * i64::from(page_size)).into());
        let items = MatchSummaryListItem::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            format!("SELECT * FROM ({MATCH_SUMMARY_SELECT}) WHERE 1=1{filter} ORDER BY confirmed_at DESC,invoice_id DESC LIMIT ? OFFSET ?"),
            page_values,
        ))
        .all(&self.connection)
        .await?;
        Ok(MatchSummariesPage {
            items,
            counts,
            total,
            page,
            page_size,
        })
    }

    pub async fn list_match_summary_attempts(
        &self,
        query: MatchSummaryAttemptsQuery,
    ) -> AppResult<MatchSummaryAttemptsPage> {
        let page = query.page.unwrap_or(1).max(1);
        let page_size = query.page_size.unwrap_or(20).clamp(1, 100);
        let status = normalized_attempt_status(query.status.as_deref())?;
        let mut filter = String::new();
        let mut values = Vec::<sea_orm::Value>::new();
        if let Some(invoice_id) = query
            .invoice_id
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
        {
            filter.push_str(" AND a.invoice_id=?");
            values.push(invoice_id.into());
        }
        if let Some(status) = status {
            filter.push_str(" AND a.status=?");
            values.push(status.into());
        }
        if let Some(search) = query
            .search
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
        {
            filter.push_str(" AND (a.invoice_id LIKE ? OR i.invoice_number LIKE ? OR a.result_summary LIKE ? OR a.error_message LIKE ?)");
            let pattern = format!("%{search}%");
            for _ in 0..4 {
                values.push(pattern.clone().into());
            }
        }
        let from = " FROM match_summary_attempts a JOIN invoices i ON i.id=a.invoice_id WHERE 1=1";
        let total = CountRow::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            format!("SELECT COUNT(*) count{from}{filter}"),
            values.clone(),
        ))
        .one(&self.connection)
        .await?
        .map_or(0, |row| row.count);
        let mut page_values = values;
        page_values.push(i64::from(page_size).into());
        page_values.push((i64::from(page - 1) * i64::from(page_size)).into());
        let rows = MatchSummaryAttemptRow::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            format!("SELECT a.id,a.invoice_id,i.invoice_number,a.input_json,a.trigger,a.provider,a.model,a.status,a.result_summary,a.error_message,a.started_at,a.completed_at{from}{filter} ORDER BY a.started_at DESC,a.id DESC LIMIT ? OFFSET ?"),
            page_values,
        ))
        .all(&self.connection)
        .await?;
        let items = rows
            .into_iter()
            .map(|row| {
                Ok(MatchSummaryAttemptItem {
                    id: row.id,
                    invoice_id: row.invoice_id,
                    invoice_number: row.invoice_number,
                    input: serde_json::from_str::<MatchSummaryInput>(&row.input_json)
                        .map_err(|error| {
                            AppError::Internal(format!("无法解析摘要尝试输入：{error}"))
                        })?
                        .into(),
                    trigger: row.trigger,
                    provider: row.provider,
                    model: row.model,
                    status: row.status,
                    result_summary: row.result_summary,
                    error_message: row.error_message,
                    started_at: row.started_at,
                    completed_at: row.completed_at,
                })
            })
            .collect::<AppResult<Vec<_>>>()?;
        Ok(MatchSummaryAttemptsPage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub async fn enqueue_match_summary(&self, invoice_id: &str) -> AppResult<()> {
        let result = self.connection.execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE invoice_matches SET summary_status='pending',summary_trigger='manual_request',summary_error=NULL,summary_provider=NULL,summary_model=NULL,\
             summary_input_hash=CASE WHEN summary_source='manual' THEN summary_input_hash ELSE NULL END,\
             summary_generated_at=CASE WHEN summary_source='manual' THEN summary_generated_at ELSE NULL END \
             WHERE invoice_id=? AND summary_status IN ('not_generated','failed','succeeded')",
            [invoice_id.into()],
        )).await?;
        if result.rows_affected() == 0 {
            return Err(AppError::InvalidInput(
                "仅未生成、失败或已完成的摘要可以手动生成".into(),
            ));
        }
        Ok(())
    }

    pub async fn enqueue_all_match_summaries(&self) -> AppResult<u64> {
        let result = self.connection.execute(Statement::from_string(
            DbBackend::Sqlite,
            "UPDATE invoice_matches SET summary_status='pending',summary_trigger='manual_batch',summary_error=NULL,summary_provider=NULL,summary_model=NULL,summary_input_hash=NULL,summary_generated_at=NULL WHERE summary_status IN ('not_generated','failed')",
        )).await?;
        Ok(result.rows_affected())
    }

    pub async fn update_match_summary(
        &self,
        invoice_id: &str,
        display_name: &str,
    ) -> AppResult<()> {
        let display_name = validate_display_name(display_name)?;
        let transaction = self.connection.begin().await?;
        transaction.execute(Statement::from_string(
            DbBackend::Sqlite,
            "DELETE FROM match_summary_attempts WHERE datetime(started_at) < datetime('now', '-180 days')",
        )).await?;
        let input = load_summary_input(&transaction, invoice_id).await?;
        let input_json = serde_json::to_string(&input)
            .map_err(|error| AppError::Internal(format!("无法序列化摘要输入：{error}")))?;
        let input_hash = hex::encode(Sha256::digest(input_json.as_bytes()));
        let completed_at = now();
        let result = transaction.execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE invoice_matches SET display_name=?,summary_status='succeeded',summary_source='manual',summary_trigger=NULL,summary_error=NULL,summary_provider=NULL,summary_model=NULL,summary_input_hash=?,summary_generated_at=? WHERE invoice_id=?",
            [display_name.clone().into(), input_hash.clone().into(), completed_at.clone().into(), invoice_id.into()],
        )).await?;
        if result.rows_affected() == 0 {
            transaction.rollback().await?;
            return Err(AppError::NotFound(format!("发票匹配 {invoice_id}")));
        }
        transaction
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "UPDATE match_summary_attempts SET status='discarded',error_message='摘要已被人工编辑替换',completed_at=? WHERE invoice_id=? AND status='processing'",
                [completed_at.clone().into(), invoice_id.into()],
            ))
            .await?;
        match_summary_attempt::ActiveModel {
            id: Set(uuid::Uuid::now_v7().to_string()),
            invoice_id: Set(invoice_id.into()),
            input_hash: Set(input_hash),
            input_json: Set(input_json),
            trigger: Set("manual_edit".into()),
            provider: Set(None),
            model: Set(None),
            status: Set("manual_edit".into()),
            result_summary: Set(Some(display_name)),
            error_message: Set(None),
            started_at: Set(completed_at.clone()),
            completed_at: Set(Some(completed_at)),
        }
        .insert(&transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn clean_expired_match_summary_attempts(&self) -> AppResult<u64> {
        let result = self.connection.execute(Statement::from_string(
            DbBackend::Sqlite,
            "DELETE FROM match_summary_attempts WHERE datetime(started_at) < datetime('now', '-180 days')",
        )).await?;
        Ok(result.rows_affected())
    }

    pub async fn activate_automatic_match_summaries(&self) -> AppResult<u64> {
        let result = self.connection.execute(Statement::from_string(
            DbBackend::Sqlite,
            "UPDATE invoice_matches SET summary_status='pending',summary_trigger=COALESCE(summary_trigger,'automatic') WHERE summary_status='not_generated' AND summary_source IS NOT 'manual'",
        )).await?;
        Ok(result.rows_affected())
    }

    pub async fn pause_pending_match_summaries(&self) -> AppResult<u64> {
        let result = self.connection.execute(Statement::from_string(
            DbBackend::Sqlite,
            "UPDATE invoice_matches SET summary_status=CASE WHEN summary_source='manual' THEN 'succeeded' ELSE 'not_generated' END,summary_trigger=NULL WHERE summary_status='pending'",
        )).await?;
        Ok(result.rows_affected())
    }

    #[cfg(test)]
    pub async fn claim_next_match_summary(&self) -> AppResult<Option<ClaimedMatchSummary>> {
        self.claim_next_match_summary_with_model("unknown", "unknown")
            .await
    }

    pub async fn claim_next_match_summary_with_model(
        &self,
        provider: &str,
        model: &str,
    ) -> AppResult<Option<ClaimedMatchSummary>> {
        let transaction = self.connection.begin().await?;
        transaction.execute(Statement::from_string(
            DbBackend::Sqlite,
            "DELETE FROM match_summary_attempts WHERE datetime(started_at) < datetime('now', '-180 days')",
        )).await?;
        let Some(summary) = invoice_match::Entity::find()
            .filter(invoice_match::Column::SummaryStatus.eq("pending"))
            .order_by_asc(invoice_match::Column::ConfirmedAt)
            .order_by_asc(invoice_match::Column::InvoiceId)
            .one(&transaction)
            .await?
        else {
            transaction.rollback().await?;
            return Ok(None);
        };
        let input = load_summary_input(&transaction, &summary.invoice_id).await?;
        let serialized = serde_json::to_vec(&input)
            .map_err(|error| AppError::Internal(format!("无法序列化摘要输入：{error}")))?;
        let input_hash = hex::encode(Sha256::digest(&serialized));
        let attempt_id = uuid::Uuid::now_v7().to_string();
        match_summary_attempt::ActiveModel {
            id: Set(attempt_id.clone()),
            invoice_id: Set(summary.invoice_id.clone()),
            input_hash: Set(input_hash.clone()),
            input_json: Set(String::from_utf8(serialized)
                .map_err(|error| AppError::Internal(format!("摘要输入不是有效 UTF-8：{error}")))?),
            trigger: Set(summary
                .summary_trigger
                .clone()
                .unwrap_or_else(|| "automatic".into())),
            provider: Set(Some(provider.into())),
            model: Set(Some(model.into())),
            status: Set("processing".into()),
            result_summary: Set(None),
            error_message: Set(None),
            started_at: Set(now()),
            completed_at: Set(None),
        }
        .insert(&transaction)
        .await?;
        let mut active: invoice_match::ActiveModel = summary.into();
        active.summary_status = Set("processing".into());
        active.summary_error = Set(None);
        active.summary_input_hash = Set(Some(input_hash.clone()));
        active.update(&transaction).await?;
        transaction.commit().await?;
        Ok(Some(ClaimedMatchSummary {
            attempt_id,
            invoice_id: input.invoice_id.clone(),
            input,
            input_hash,
        }))
    }

    pub async fn complete_match_summary(
        &self,
        invoice_id: &str,
        input_hash: &str,
        display_name: &str,
        provider: &str,
        model: &str,
    ) -> AppResult<bool> {
        let transaction = self.connection.begin().await?;
        let completed_at = now();
        let result = transaction
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "UPDATE invoice_matches SET display_name=?,summary_status='succeeded',summary_error=NULL,\
                 summary_source='generated',summary_trigger=NULL,summary_provider=?,summary_model=?,summary_generated_at=? \
                 WHERE invoice_id=? AND summary_status='processing' AND summary_input_hash=?",
                [
                    display_name.into(),
                    provider.into(),
                    model.into(),
                    completed_at.clone().into(),
                    invoice_id.into(),
                    input_hash.into(),
                ],
            ))
            .await?;
        if result.rows_affected() == 1 {
            transaction.execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "UPDATE match_summary_attempts SET status='succeeded',result_summary=?,completed_at=? WHERE invoice_id=? AND input_hash=? AND status='processing'",
                [display_name.into(), completed_at.into(), invoice_id.into(), input_hash.into()],
            )).await?;
        } else {
            transaction.execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "UPDATE match_summary_attempts SET status='discarded',error_message='摘要输入或当前状态已变化',completed_at=? WHERE invoice_id=? AND input_hash=? AND status='processing'",
                [completed_at.into(), invoice_id.into(), input_hash.into()],
            )).await?;
        }
        transaction.commit().await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn fail_match_summary(
        &self,
        invoice_id: &str,
        input_hash: &str,
        error: &str,
        provider: &str,
        model: &str,
    ) -> AppResult<Option<String>> {
        let transaction = self.connection.begin().await?;
        let completed_at = now();
        let bounded_error = error.chars().take(500).collect::<String>();
        let result = transaction
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "UPDATE invoice_matches SET display_name=CASE WHEN summary_source='manual' THEN display_name ELSE NULL END,\
                 summary_status=CASE WHEN summary_source='manual' THEN 'succeeded' ELSE 'failed' END,\
                 summary_error=CASE WHEN summary_source='manual' THEN summary_error ELSE ? END,summary_trigger=NULL,\
                 summary_provider=CASE WHEN summary_source='manual' THEN summary_provider ELSE ? END,\
                 summary_model=CASE WHEN summary_source='manual' THEN summary_model ELSE ? END,\
                 summary_input_hash=CASE WHEN summary_source='manual' THEN NULL ELSE summary_input_hash END,\
                 summary_generated_at=CASE WHEN summary_source='manual' THEN summary_generated_at ELSE ? END \
                 WHERE invoice_id=? AND summary_status='processing' AND summary_input_hash=?",
                [
                    bounded_error.clone().into(),
                    provider.into(),
                    model.into(),
                    completed_at.clone().into(),
                    invoice_id.into(),
                    input_hash.into(),
                ],
            ))
            .await?;
        transaction.execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE match_summary_attempts SET status=CASE WHEN ? THEN 'failed' ELSE 'discarded' END,error_message=?,completed_at=? WHERE invoice_id=? AND input_hash=? AND status='processing'",
            [(result.rows_affected() == 1).into(), bounded_error.into(), completed_at.into(), invoice_id.into(), input_hash.into()],
        )).await?;
        let status = if result.rows_affected() == 1 {
            invoice_match::Entity::find_by_id(invoice_id)
                .one(&transaction)
                .await?
                .map(|model| model.summary_status)
        } else {
            None
        };
        transaction.commit().await?;
        Ok(status)
    }

    pub async fn retry_match_summary(&self, invoice_id: &str) -> AppResult<()> {
        let result = self
            .connection
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "UPDATE invoice_matches SET summary_status='pending',summary_trigger='retry',summary_error=NULL,\
                 summary_provider=NULL,summary_model=NULL,summary_input_hash=NULL,summary_generated_at=NULL \
                 WHERE invoice_id=? AND summary_status='failed'",
                [invoice_id.into()],
            ))
            .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::InvalidInput(
                "仅生成失败的确认组合可以重试商品摘要".into(),
            ));
        }
        Ok(())
    }
}

fn normalized_summary_status(value: Option<&str>) -> AppResult<Option<&str>> {
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

fn normalized_attempt_status(value: Option<&str>) -> AppResult<Option<&str>> {
    match value
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "all")
    {
        None => Ok(None),
        Some(
            value @ ("processing" | "succeeded" | "failed" | "interrupted" | "discarded"
            | "manual_edit"),
        ) => Ok(Some(value)),
        Some(_) => Err(AppError::InvalidInput("摘要记录状态筛选无效".into())),
    }
}

fn validate_display_name(value: &str) -> AppResult<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::InvalidInput("商品摘要不能为空".into()));
    }
    if value.contains(['\r', '\n']) {
        return Err(AppError::InvalidInput("商品摘要只能包含一行".into()));
    }
    if value.chars().count() > 60 {
        return Err(AppError::InvalidInput("商品摘要最多 60 个字符".into()));
    }
    Ok(value.into())
}

pub(crate) async fn reset_match_summary<C>(connection: &C, invoice_id: &str) -> AppResult<bool>
where
    C: sea_orm::ConnectionTrait,
{
    let result = connection
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE invoice_matches SET \
             display_name=CASE WHEN summary_source='manual' THEN display_name ELSE NULL END,\
             summary_status=CASE WHEN summary_source='manual' THEN 'succeeded' ELSE 'not_generated' END,\
             summary_source=CASE WHEN summary_source='manual' THEN summary_source ELSE NULL END,\
             summary_trigger=NULL,summary_error=NULL,summary_provider=NULL,summary_model=NULL,\
             summary_input_hash=NULL,\
             summary_generated_at=CASE WHEN summary_source='manual' THEN summary_generated_at ELSE NULL END \
             WHERE invoice_id=? AND (summary_source IS NOT 'manual' OR summary_status IN ('pending','processing'))",
            [invoice_id.into()],
        ))
        .await?;
    if result.rows_affected() == 1 {
        connection
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "UPDATE match_summary_attempts SET status='discarded',error_message='摘要输入已变化',completed_at=? WHERE invoice_id=? AND status='processing'",
                [now().into(), invoice_id.into()],
            ))
            .await?;
    }
    Ok(result.rows_affected() == 1)
}

async fn load_summary_input<C>(connection: &C, invoice_id: &str) -> AppResult<MatchSummaryInput>
where
    C: sea_orm::ConnectionTrait,
{
    let invoice = invoice::Entity::find_by_id(invoice_id)
        .one(connection)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("发票 {invoice_id}")))?;
    let invoice_items = invoice_line_item::Entity::find()
        .filter(invoice_line_item::Column::InvoiceId.eq(invoice_id))
        .order_by_asc(invoice_line_item::Column::Position)
        .all(connection)
        .await?
        .into_iter()
        .map(|item| MatchSummaryInvoiceItem {
            name: item.name,
            specification: item.specification,
            unit: item.unit,
            quantity: item.quantity,
            unit_price_fen: item.unit_price_fen,
            amount_fen: item.amount_fen,
            tax_rate: item.tax_rate,
            tax_fen: item.tax_fen,
        })
        .collect();
    let links = invoice_match_order::Entity::find()
        .filter(invoice_match_order::Column::InvoiceId.eq(invoice_id))
        .order_by_asc(invoice_match_order::Column::Position)
        .all(connection)
        .await?;
    let mut orders = Vec::with_capacity(links.len());
    for link in links {
        let value = order::Entity::find_by_id(&link.order_id)
            .one(connection)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("订单 {}", link.order_id)))?;
        let product_names = order_item::Entity::find()
            .filter(order_item::Column::OrderId.eq(&link.order_id))
            .order_by_asc(order_item::Column::Position)
            .all(connection)
            .await?
            .into_iter()
            .map(|item| item.product_name)
            .collect();
        orders.push(MatchSummaryOrder {
            shop_name: value.shop_name,
            product_names,
        });
    }
    Ok(MatchSummaryInput {
        invoice_id: invoice_id.to_owned(),
        seller_name: invoice.seller_name,
        invoice_items,
        orders,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ConfirmInvoiceMatchRequest, UpdateInvoiceRequest, UpdateOrderRequest};

    async fn seeded_match() -> Database {
        let database = Database::in_memory().await.unwrap();
        database.execute_test_statement(
            "INSERT INTO import_batches (id,platform,status,created_at) VALUES ('ob','taobao','completed','now');
             INSERT INTO import_tasks (id,batch_id,original_file_name,image_sha256,status,retry_count,created_at) VALUES ('o','ob','o.png','oh','succeeded',0,'now');
             INSERT INTO orders (id,platform,external_order_number,paid_amount_fen,shop_name,raw_ocr_json,created_at,updated_at) VALUES ('o','taobao','ORDER-1',1000,'订单店铺','{}','now','now');
             INSERT INTO order_items (order_id,position,product_name) VALUES ('o',0,'数据线'),('o',1,'充电器');
             INSERT INTO invoice_batches (id,status,created_at) VALUES ('ib','completed','now');
             INSERT INTO invoice_tasks (id,batch_id,original_file_name,source_sha256,media_type,status,retry_count,created_at) VALUES ('it','ib','i.pdf','ih','application/pdf','succeeded',0,'now');
             INSERT INTO invoices (id,task_id,type,subtype,type_description,subtype_description,seller_name,total_fen,currency,preview_path,raw_invoice_json,created_at,updated_at) VALUES ('i','it',3,'VatInvoice','发票','发票','发票销售方',1000,'CNY','i.pdf','{}','now','now');
             INSERT INTO invoice_items (invoice_id,position,name,specification,quantity,amount_fen,raw_json) VALUES ('i',0,'充电设备','USB-C','1',1000,'{}');"
        ).await.unwrap();
        database
            .confirm_invoice_match(ConfirmInvoiceMatchRequest {
                invoice_id: "i".into(),
                order_ids: vec!["o".into()],
                source: "automatic".into(),
                accept_mismatches: false,
            })
            .await
            .unwrap();
        database
    }

    #[test]
    fn display_name_prefers_generated_then_products_then_order_labels() {
        assert_eq!(
            display_name(
                Some(" 数据线与充电器 "),
                &["旧商品".into()],
                &["O-1".into()]
            ),
            "数据线与充电器"
        );
        assert_eq!(
            display_name(None, &["数据线".into(), "充电器".into()], &["O-1".into()]),
            "数据线、充电器"
        );
        assert_eq!(display_name(None, &[], &["O-1".into()]), "O-1");
        assert_eq!(display_name(None, &[], &[]), "订单组合");
    }

    #[tokio::test]
    async fn claim_contains_complete_inputs_and_success_is_hash_guarded() {
        let database = seeded_match().await;
        let claimed = database.claim_next_match_summary().await.unwrap().unwrap();
        assert_eq!(claimed.input.seller_name.as_deref(), Some("发票销售方"));
        assert_eq!(
            claimed.input.invoice_items[0].specification.as_deref(),
            Some("USB-C")
        );
        assert_eq!(
            claimed.input.orders[0].shop_name.as_deref(),
            Some("订单店铺")
        );
        assert_eq!(claimed.input.orders[0].product_names, ["数据线", "充电器"]);
        assert!(
            !database
                .complete_match_summary("i", "stale", "错误摘要", "ollama", "qwen")
                .await
                .unwrap()
        );
        assert!(
            database
                .complete_match_summary(
                    "i",
                    &claimed.input_hash,
                    "数据线与充电器",
                    "ollama",
                    "qwen"
                )
                .await
                .unwrap()
        );
        let value = invoice_match::Entity::find_by_id("i")
            .one(&database.connection)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(value.summary_status, "succeeded");
        assert_eq!(value.display_name.as_deref(), Some("数据线与充电器"));
        assert_eq!(value.summary_model.as_deref(), Some("qwen"));
    }

    #[tokio::test]
    async fn failure_requires_explicit_retry_and_reset_invalidates_in_flight_hash() {
        let database = seeded_match().await;
        let claimed = database.claim_next_match_summary().await.unwrap().unwrap();
        assert_eq!(
            database
                .fail_match_summary("i", &claimed.input_hash, "模型离线", "openai", "model")
                .await
                .unwrap()
                .as_deref(),
            Some("failed")
        );
        assert!(database.claim_next_match_summary().await.unwrap().is_none());
        database.retry_match_summary("i").await.unwrap();
        let retried = database.claim_next_match_summary().await.unwrap().unwrap();
        reset_match_summary(&database.connection, "i")
            .await
            .unwrap();
        assert!(
            !database
                .complete_match_summary("i", &retried.input_hash, "过期摘要", "openai", "model")
                .await
                .unwrap()
        );
        let value = invoice_match::Entity::find_by_id("i")
            .one(&database.connection)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(value.summary_status, "not_generated");
        assert!(value.display_name.is_none());
    }

    #[tokio::test]
    async fn changing_order_products_resets_a_successful_summary() {
        let database = seeded_match().await;
        database.activate_automatic_match_summaries().await.unwrap();
        let claimed = database.claim_next_match_summary().await.unwrap().unwrap();
        database
            .complete_match_summary("i", &claimed.input_hash, "数据线与充电器", "ollama", "qwen")
            .await
            .unwrap();

        database
            .update_order(UpdateOrderRequest {
                id: "o".into(),
                external_order_number: Some("ORDER-1".into()),
                product_names: vec![" 扩展坞 ".into()],
                paid_amount_fen: Some(1000),
                ordered_at: None,
                shop_name: Some("订单店铺".into()),
            })
            .await
            .unwrap();

        database.activate_automatic_match_summaries().await.unwrap();
        let claimed = database.claim_next_match_summary().await.unwrap().unwrap();
        assert_eq!(claimed.input.orders[0].product_names, ["扩展坞"]);
        assert!(
            invoice_match::Entity::find_by_id("i")
                .one(&database.connection)
                .await
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    async fn changing_invoice_seller_resets_a_successful_summary() {
        let database = seeded_match().await;
        let claimed = database.claim_next_match_summary().await.unwrap().unwrap();
        database
            .complete_match_summary(
                "i",
                &claimed.input_hash,
                "数据线与充电器",
                "openai",
                "shared-model",
            )
            .await
            .unwrap();

        database
            .update_invoice(UpdateInvoiceRequest {
                id: "i".into(),
                invoice_code: None,
                invoice_number: None,
                invoice_date: None,
                buyer_name: None,
                buyer_tax_id: None,
                seller_name: Some("新销售方".into()),
                seller_tax_id: None,
                amount_fen: None,
                tax_fen: None,
                total_fen: Some(1000),
                currency: "CNY".into(),
            })
            .await
            .unwrap();

        database.activate_automatic_match_summaries().await.unwrap();
        let claimed = database.claim_next_match_summary().await.unwrap().unwrap();
        assert_eq!(claimed.input.seller_name.as_deref(), Some("新销售方"));
    }

    #[tokio::test]
    async fn manual_summary_survives_input_changes_and_failed_regeneration() {
        let database = seeded_match().await;
        database
            .update_match_summary("i", "人工确认摘要")
            .await
            .unwrap();

        database
            .update_order(UpdateOrderRequest {
                id: "o".into(),
                external_order_number: Some("ORDER-1".into()),
                product_names: vec!["新商品".into()],
                paid_amount_fen: Some(1000),
                ordered_at: None,
                shop_name: Some("订单店铺".into()),
            })
            .await
            .unwrap();
        let manual = invoice_match::Entity::find_by_id("i")
            .one(&database.connection)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(manual.display_name.as_deref(), Some("人工确认摘要"));
        assert_eq!(manual.summary_source.as_deref(), Some("manual"));
        assert_eq!(manual.summary_status, "succeeded");
        let manual_generated_at = manual.summary_generated_at.clone();

        database.enqueue_match_summary("i").await.unwrap();
        let claimed = database.claim_next_match_summary().await.unwrap().unwrap();
        assert_eq!(
            database
                .fail_match_summary("i", &claimed.input_hash, "模型离线", "openai", "model")
                .await
                .unwrap()
                .as_deref(),
            Some("succeeded")
        );
        let retained = invoice_match::Entity::find_by_id("i")
            .one(&database.connection)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retained.display_name.as_deref(), Some("人工确认摘要"));
        assert_eq!(retained.summary_status, "succeeded");
        assert_eq!(retained.summary_source.as_deref(), Some("manual"));
        assert_eq!(retained.summary_error, None);
        assert_eq!(retained.summary_provider, None);
        assert_eq!(retained.summary_model, None);
        assert_eq!(retained.summary_generated_at, manual_generated_at);
        assert_eq!(retained.summary_input_hash, None);

        let attempts = match_summary_attempt::Entity::find()
            .filter(match_summary_attempt::Column::InvoiceId.eq("i"))
            .order_by_asc(match_summary_attempt::Column::StartedAt)
            .all(&database.connection)
            .await
            .unwrap();
        assert_eq!(attempts.len(), 2);
        assert_eq!(attempts[0].status, "manual_edit");
        assert_eq!(attempts[1].status, "failed");
    }

    #[tokio::test]
    async fn mode_transitions_pause_and_activate_only_model_owned_work() {
        let database = seeded_match().await;
        assert_eq!(database.pause_pending_match_summaries().await.unwrap(), 1);
        let paused = invoice_match::Entity::find_by_id("i")
            .one(&database.connection)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(paused.summary_status, "not_generated");

        assert_eq!(
            database.activate_automatic_match_summaries().await.unwrap(),
            1
        );
        assert!(database.claim_next_match_summary().await.unwrap().is_some());
    }

    #[tokio::test]
    async fn input_change_discards_in_flight_manual_regeneration() {
        let database = seeded_match().await;
        database
            .update_match_summary("i", "人工确认摘要")
            .await
            .unwrap();
        database.enqueue_match_summary("i").await.unwrap();
        let claimed = database.claim_next_match_summary().await.unwrap().unwrap();

        database
            .update_order(UpdateOrderRequest {
                id: "o".into(),
                external_order_number: Some("ORDER-1".into()),
                product_names: vec!["新商品".into()],
                paid_amount_fen: Some(1000),
                ordered_at: None,
                shop_name: Some("订单店铺".into()),
            })
            .await
            .unwrap();

        assert!(
            !database
                .complete_match_summary("i", &claimed.input_hash, "过期生成摘要", "ollama", "qwen")
                .await
                .unwrap()
        );
        let retained = invoice_match::Entity::find_by_id("i")
            .one(&database.connection)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retained.display_name.as_deref(), Some("人工确认摘要"));
        assert_eq!(retained.summary_status, "succeeded");
        assert_eq!(retained.summary_source.as_deref(), Some("manual"));
        let generated_attempt = match_summary_attempt::Entity::find_by_id(&claimed.attempt_id)
            .one(&database.connection)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(generated_attempt.status, "discarded");
    }

    #[tokio::test]
    async fn summary_attempt_history_returns_snapshots_and_cleans_expired_rows() {
        let database = seeded_match().await;
        database
            .update_match_summary("i", "人工摘要")
            .await
            .unwrap();
        database.execute_test_statement(
            "INSERT INTO match_summary_attempts (id,invoice_id,input_hash,input_json,trigger,status,started_at,completed_at) VALUES ('old','i','old','{\"seller_name\":null,\"invoice_items\":[],\"orders\":[]}','manual_edit','manual_edit','2000-01-01T00:00:00Z','2000-01-01T00:00:00Z')",
        ).await.unwrap();

        assert_eq!(
            database
                .clean_expired_match_summary_attempts()
                .await
                .unwrap(),
            1
        );
        let page = database
            .list_match_summary_attempts(MatchSummaryAttemptsQuery {
                invoice_id: Some("i".into()),
                page: None,
                page_size: None,
                search: None,
                status: None,
            })
            .await
            .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(
            page.items[0].input.seller_name.as_deref(),
            Some("发票销售方")
        );
        assert_eq!(page.items[0].result_summary.as_deref(), Some("人工摘要"));
    }
}
