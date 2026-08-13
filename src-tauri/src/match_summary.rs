use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, QueryFilter,
    QueryOrder, Set, Statement, TransactionTrait,
};
use sha2::{Digest, Sha256};

use crate::db::{Database, now};
use crate::entities::{
    invoice, invoice_line_item, invoice_match, invoice_match_order, order, order_item,
};
use crate::error::{AppError, AppResult};
use crate::models::{
    ClaimedMatchSummary, MatchSummaryInput, MatchSummaryInvoiceItem, MatchSummaryOrder,
};

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
    pub async fn claim_next_match_summary(&self) -> AppResult<Option<ClaimedMatchSummary>> {
        let transaction = self.connection.begin().await?;
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
        let input_hash = hex::encode(Sha256::digest(serialized));
        let mut active: invoice_match::ActiveModel = summary.into();
        active.summary_status = Set("processing".into());
        active.summary_error = Set(None);
        active.summary_input_hash = Set(Some(input_hash.clone()));
        active.update(&transaction).await?;
        transaction.commit().await?;
        Ok(Some(ClaimedMatchSummary {
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
        let result = self
            .connection
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "UPDATE invoice_matches SET display_name=?,summary_status='succeeded',summary_error=NULL,\
                 summary_provider=?,summary_model=?,summary_generated_at=? \
                 WHERE invoice_id=? AND summary_status='processing' AND summary_input_hash=?",
                [
                    display_name.into(),
                    provider.into(),
                    model.into(),
                    now().into(),
                    invoice_id.into(),
                    input_hash.into(),
                ],
            ))
            .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn fail_match_summary(
        &self,
        invoice_id: &str,
        input_hash: &str,
        error: &str,
        provider: &str,
        model: &str,
    ) -> AppResult<bool> {
        let result = self
            .connection
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "UPDATE invoice_matches SET display_name=NULL,summary_status='failed',summary_error=?,\
                 summary_provider=?,summary_model=?,summary_generated_at=? \
                 WHERE invoice_id=? AND summary_status='processing' AND summary_input_hash=?",
                [
                    error.chars().take(500).collect::<String>().into(),
                    provider.into(),
                    model.into(),
                    now().into(),
                    invoice_id.into(),
                    input_hash.into(),
                ],
            ))
            .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn retry_match_summary(&self, invoice_id: &str) -> AppResult<()> {
        let result = self
            .connection
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "UPDATE invoice_matches SET display_name=NULL,summary_status='pending',summary_error=NULL,\
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

pub(crate) async fn reset_match_summary<C>(connection: &C, invoice_id: &str) -> AppResult<bool>
where
    C: sea_orm::ConnectionTrait,
{
    let result = connection
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE invoice_matches SET display_name=NULL,summary_status='pending',summary_error=NULL,\
             summary_provider=NULL,summary_model=NULL,summary_input_hash=NULL,summary_generated_at=NULL \
             WHERE invoice_id=?",
            [invoice_id.into()],
        ))
        .await?;
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
        assert!(
            database
                .fail_match_summary("i", &claimed.input_hash, "模型离线", "openai", "model")
                .await
                .unwrap()
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
        assert_eq!(value.summary_status, "pending");
        assert!(value.display_name.is_none());
    }

    #[tokio::test]
    async fn changing_order_products_resets_a_successful_summary() {
        let database = seeded_match().await;
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

        let claimed = database.claim_next_match_summary().await.unwrap().unwrap();
        assert_eq!(claimed.input.seller_name.as_deref(), Some("新销售方"));
    }
}
