use std::collections::HashSet;

use sea_orm::{
    ActiveModelTrait, ActiveValue::NotSet, ColumnTrait, ConnectionTrait, DbBackend, EntityTrait,
    FromQueryResult, IntoActiveModel, ModelTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, Set, Statement, TransactionTrait,
};

use crate::db::{Database, now};
use crate::entities::{
    invoice, invoice_batch, invoice_line_item, invoice_match, invoice_recovery_marker,
    invoice_source_link, invoice_task, project_invoice,
};
use crate::projects::{compact_project, touch_project};

#[derive(Debug)]
pub enum PersistInvoicesOutcome {
    Inserted { invoice_ids: HashSet<String> },
    Duplicate { existing_invoice_id: String },
    RecoveryNoMatch { existing_invoice_id: Option<String> },
}
use crate::error::{AppError, AppResult};
use crate::models::{
    InvoiceBatchCounts, InvoiceBatchDetail, InvoiceBatchSummary, InvoiceBatchesPage,
    InvoiceBatchesQuery, InvoiceDetail, InvoiceLineItem, InvoiceListItem, InvoiceSort, InvoiceTask,
    InvoicesPage, InvoicesQuery, NormalizedInvoice, SortDirection, UpdateInvoiceRequest,
};

impl Database {
    pub async fn invoice_hash_exists(&self, hash: &str) -> AppResult<bool> {
        let recovery_available = invoice_recovery_marker::Entity::find()
            .filter(invoice_recovery_marker::Column::SourceSha256.eq(hash))
            .filter(invoice_recovery_marker::Column::RecoveryTaskId.is_null())
            .one(&self.connection)
            .await?
            .is_some();
        if recovery_available {
            return Ok(false);
        }
        Ok(invoice_task::Entity::find()
            .filter(invoice_task::Column::SourceSha256.eq(hash))
            .filter(invoice_task::Column::SourcePath.is_not_null())
            .one(&self.connection)
            .await?
            .is_some())
    }

    pub async fn create_invoice_batch(&self, id: &str, forced_type: Option<i32>) -> AppResult<()> {
        invoice_batch::ActiveModel {
            id: Set(id.to_owned()),
            status: Set("queued".into()),
            forced_type: Set(forced_type),
            created_at: Set(now()),
            completed_at: Set(None),
        }
        .insert(&self.connection)
        .await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_invoice_task(
        &self,
        id: &str,
        batch_id: &str,
        original_file_name: &str,
        source_path: Option<&str>,
        hash: &str,
        media_type: &str,
        forced_type: Option<i32>,
        status: &str,
        error_code: Option<&str>,
        error_message: Option<&str>,
    ) -> AppResult<()> {
        invoice_task::ActiveModel {
            id: Set(id.to_owned()),
            batch_id: Set(batch_id.to_owned()),
            original_file_name: Set(original_file_name.to_owned()),
            source_path: Set(source_path.map(str::to_owned)),
            source_sha256: Set(hash.to_owned()),
            media_type: Set(media_type.to_owned()),
            forced_type: Set(forced_type),
            status: Set(status.to_owned()),
            error_code: Set(error_code.map(str::to_owned)),
            error_message: Set(error_message.map(str::to_owned)),
            warning_message: Set(None),
            retry_count: Set(0),
            request_id: Set(None),
            raw_response_json: Set(None),
            existing_invoice_id: Set(None),
            created_at: Set(now()),
            started_at: Set(None),
            completed_at: Set((status == "failed").then(now)),
        }
        .insert(&self.connection)
        .await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_invoice_source_task(
        &self,
        id: &str,
        batch_id: &str,
        original_file_name: &str,
        source_path: &str,
        hash: &str,
        media_type: &str,
        forced_type: Option<i32>,
    ) -> AppResult<bool> {
        let transaction = self.connection.begin().await?;
        invoice_task::ActiveModel {
            id: Set(id.to_owned()),
            batch_id: Set(batch_id.to_owned()),
            original_file_name: Set(original_file_name.to_owned()),
            source_path: Set(None),
            source_sha256: Set(hash.to_owned()),
            media_type: Set(media_type.to_owned()),
            forced_type: Set(forced_type),
            status: Set("queued".into()),
            error_code: Set(None),
            error_message: Set(None),
            warning_message: Set(None),
            retry_count: Set(0),
            request_id: Set(None),
            raw_response_json: Set(None),
            existing_invoice_id: Set(None),
            created_at: Set(now()),
            started_at: Set(None),
            completed_at: Set(None),
        }
        .insert(&transaction)
        .await?;

        let claimed = invoice_recovery_marker::Entity::update_many()
            .col_expr(
                invoice_recovery_marker::Column::RecoveryTaskId,
                sea_orm::sea_query::Expr::value(Some(id.to_owned())),
            )
            .filter(invoice_recovery_marker::Column::SourceSha256.eq(hash))
            .filter(invoice_recovery_marker::Column::RecoveryTaskId.is_null())
            .exec(&transaction)
            .await?
            .rows_affected;
        if claimed == 0
            && invoice_task::Entity::find()
                .filter(invoice_task::Column::SourceSha256.eq(hash))
                .filter(invoice_task::Column::SourcePath.is_not_null())
                .one(&transaction)
                .await?
                .is_some()
        {
            transaction.rollback().await?;
            return Ok(false);
        }

        invoice_task::Entity::update_many()
            .col_expr(
                invoice_task::Column::SourcePath,
                sea_orm::sea_query::Expr::value(Some(source_path.to_owned())),
            )
            .filter(invoice_task::Column::Id.eq(id))
            .exec(&transaction)
            .await?;
        transaction.commit().await?;
        Ok(true)
    }

    pub async fn delete_invoice_batch(&self, id: &str) -> AppResult<()> {
        invoice_batch::Entity::delete_by_id(id)
            .exec(&self.connection)
            .await?;
        Ok(())
    }

    pub async fn release_invoice_recovery_claims(&self, task_id: &str) -> AppResult<()> {
        invoice_recovery_marker::Entity::update_many()
            .col_expr(
                invoice_recovery_marker::Column::RecoveryTaskId,
                sea_orm::sea_query::Expr::value(Option::<String>::None),
            )
            .filter(invoice_recovery_marker::Column::RecoveryTaskId.eq(task_id))
            .exec(&self.connection)
            .await?;
        Ok(())
    }

    pub async fn invoice_task(&self, id: &str) -> AppResult<InvoiceTask> {
        invoice_task::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .map(InvoiceTask::from)
            .ok_or_else(|| AppError::NotFound(format!("发票任务 {id}")))
    }

    pub async fn mark_invoice_processing(&self, id: &str) -> AppResult<bool> {
        let result = self.connection.execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE invoice_tasks SET status = 'processing', error_code = NULL, error_message = NULL, warning_message = NULL, started_at = ?, completed_at = NULL WHERE id = ? AND status = 'queued'",
            [now().into(), id.into()],
        )).await?;
        if result.rows_affected() == 1 {
            let task = self.invoice_task(id).await?;
            self.set_invoice_batch_processing(&task.batch_id).await?;
        }
        Ok(result.rows_affected() == 1)
    }

    pub async fn mark_invoice_failed(
        &self,
        id: &str,
        code: &str,
        message: &str,
        request_id: Option<&str>,
        raw_response: Option<&str>,
        existing_invoice_id: Option<&str>,
    ) -> AppResult<()> {
        let task = self.invoice_task(id).await?;
        self.connection.execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE invoice_tasks SET status = 'failed', error_code = ?, error_message = ?, request_id = ?, raw_response_json = ?, existing_invoice_id = ?, completed_at = ? WHERE id = ?",
            [code.into(), message.into(), request_id.into(), raw_response.into(), existing_invoice_id.into(), now().into(), id.into()],
        )).await?;
        self.refresh_invoice_batch_status(&task.batch_id).await
    }

    pub async fn mark_invoice_awaiting_review(
        &self,
        id: &str,
        request_id: &str,
        raw_response: &str,
        warning: &str,
    ) -> AppResult<()> {
        let task = self.invoice_task(id).await?;
        self.connection.execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE invoice_tasks SET status = 'awaiting_review', request_id = ?, raw_response_json = ?, warning_message = ?, completed_at = ? WHERE id = ?",
            [request_id.into(), raw_response.into(), warning.into(), now().into(), id.into()],
        )).await?;
        self.refresh_invoice_batch_status(&task.batch_id).await
    }

    pub async fn cancel_invoice_task(&self, id: &str) -> AppResult<bool> {
        let result = self.connection.execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE invoice_tasks SET status = 'canceled', completed_at = ? WHERE id = ? AND status = 'queued'",
            [now().into(), id.into()],
        )).await?;
        if result.rows_affected() == 1 {
            let task = self.invoice_task(id).await?;
            self.refresh_invoice_batch_status(&task.batch_id).await?;
        }
        Ok(result.rows_affected() == 1)
    }

    pub async fn prepare_invoice_retry(
        &self,
        id: &str,
        forced_type: Option<i32>,
    ) -> AppResult<InvoiceTask> {
        let current = invoice_task::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("发票任务 {id}")))?;
        if current.source_path.is_none() {
            return Err(AppError::InvalidInput(
                "该任务没有可重新识别的源文件".into(),
            ));
        }
        let result = self.connection.execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE invoice_tasks SET status = 'queued', forced_type = ?, error_code = NULL, error_message = NULL, warning_message = NULL, request_id = NULL, raw_response_json = NULL, existing_invoice_id = NULL, retry_count = retry_count + 1, started_at = NULL, completed_at = NULL WHERE id = ? AND status IN ('failed', 'interrupted', 'canceled')",
            [forced_type.into(), id.into()],
        )).await?;
        if result.rows_affected() != 1 {
            return Err(AppError::InvalidInput(
                "只有失败、已中断或已取消的发票任务可以重试".into(),
            ));
        }
        let task = self.invoice_task(id).await?;
        self.set_invoice_batch_processing(&task.batch_id).await?;
        Ok(task)
    }

    pub async fn persist_invoices(
        &self,
        task_id: &str,
        request_id: &str,
        raw_response: &str,
        normalized: &[NormalizedInvoice],
        warning: Option<&str>,
    ) -> AppResult<PersistInvoicesOutcome> {
        let transaction = self.connection.begin().await?;
        let recovery_markers = invoice_recovery_marker::Entity::find()
            .filter(invoice_recovery_marker::Column::RecoveryTaskId.eq(task_id))
            .all(&transaction)
            .await?;
        let is_recovery = !recovery_markers.is_empty();
        let recovery_identities = recovery_markers
            .iter()
            .map(|marker| marker.recovery_identity.as_str())
            .collect::<HashSet<_>>();

        if !is_recovery {
            for value in normalized {
                if let Some(identity) = value.reliable_identity.as_deref()
                    && let Some(existing) = invoice::Entity::find()
                        .filter(invoice::Column::ReliableIdentity.eq(identity))
                        .one(&transaction)
                        .await?
                {
                    transaction.rollback().await?;
                    return Ok(PersistInvoicesOutcome::Duplicate {
                        existing_invoice_id: existing.id,
                    });
                }
            }
        }
        let timestamp = now();
        let mut inserted_ids = HashSet::new();
        let mut matched_recoveries = HashSet::new();
        let mut skipped_existing = 0;
        let mut first_existing = None;
        for value in normalized {
            let identity = value.reliable_identity.as_deref();
            let recovery_identity =
                invoice_recovery_identity(&value.subtype, value.invoice_number.as_deref());
            if is_recovery
                && recovery_identity
                    .as_deref()
                    .is_none_or(|identity| !recovery_identities.contains(identity))
            {
                continue;
            }
            if is_recovery {
                let existing = if let Some(identity) = identity {
                    invoice::Entity::find()
                        .filter(invoice::Column::ReliableIdentity.eq(identity))
                        .one(&transaction)
                        .await?
                } else {
                    invoice::Entity::find()
                        .filter(invoice::Column::Subtype.eq(&value.subtype))
                        .filter(
                            invoice::Column::InvoiceNumber
                                .eq(value.invoice_number.as_deref().unwrap_or_default()),
                        )
                        .one(&transaction)
                        .await?
                };
                if let Some(existing) = existing {
                    matched_recoveries.insert(recovery_identity.unwrap());
                    skipped_existing += 1;
                    first_existing.get_or_insert(existing.id);
                    continue;
                }
            }
            let possible_duplicate = if value.reliable_identity.is_none() {
                if let Some(number) = value
                    .invoice_number
                    .as_deref()
                    .filter(|number| !number.trim().is_empty())
                {
                    let existing = invoice::Entity::find()
                        .filter(invoice::Column::Subtype.eq(&value.subtype))
                        .filter(invoice::Column::InvoiceNumber.eq(number))
                        .one(&transaction)
                        .await?;
                    if existing.is_some() {
                        invoice::Entity::update_many()
                            .col_expr(
                                invoice::Column::PossibleDuplicate,
                                sea_orm::sea_query::Expr::value(true),
                            )
                            .filter(invoice::Column::Subtype.eq(&value.subtype))
                            .filter(invoice::Column::InvoiceNumber.eq(number))
                            .exec(&transaction)
                            .await?;
                        true
                    } else {
                        value.possible_duplicate
                    }
                } else {
                    value.possible_duplicate
                }
            } else {
                value.possible_duplicate
            };
            let insertion = invoice::ActiveModel {
                id: Set(value.id.clone()),
                task_id: Set(task_id.to_owned()),
                r#type: Set(value.type_id),
                subtype: Set(value.subtype.clone()),
                type_description: Set(value.type_description.clone()),
                subtype_description: Set(value.subtype_description.clone()),
                invoice_code: Set(value.invoice_code.clone()),
                invoice_number: Set(value.invoice_number.clone()),
                invoice_date: Set(value.invoice_date.clone()),
                buyer_name: Set(value.buyer_name.clone()),
                buyer_tax_id: Set(value.buyer_tax_id.clone()),
                seller_name: Set(value.seller_name.clone()),
                seller_tax_id: Set(value.seller_tax_id.clone()),
                amount_fen: Set(value.amount_fen),
                tax_fen: Set(value.tax_fen),
                total_fen: Set(value.total_fen),
                currency: Set(value.currency.clone()),
                reliable_identity: Set(value.reliable_identity.clone()),
                possible_duplicate: Set(possible_duplicate),
                preview_path: Set(value.preview_path.clone()),
                page_start: Set(value.page_start),
                page_end: Set(value.page_end),
                raw_invoice_json: Set(value.raw_invoice_json.clone()),
                created_at: Set(timestamp.clone()),
                updated_at: Set(timestamp.clone()),
            }
            .insert(&transaction)
            .await;
            if let Err(error) = insertion {
                let identity = value.reliable_identity.clone();
                transaction.rollback().await?;
                if let Some(identity) = identity
                    && let Some(existing) = invoice::Entity::find()
                        .filter(invoice::Column::ReliableIdentity.eq(identity))
                        .one(&self.connection)
                        .await?
                {
                    return Ok(PersistInvoicesOutcome::Duplicate {
                        existing_invoice_id: existing.id,
                    });
                }
                return Err(error.into());
            }
            inserted_ids.insert(value.id.clone());
            if is_recovery && let Some(recovery_identity) = recovery_identity {
                matched_recoveries.insert(recovery_identity);
            }
            if !value.items.is_empty() {
                invoice_line_item::Entity::insert_many(value.items.iter().enumerate().map(
                    |(position, item)| invoice_line_item::ActiveModel {
                        id: NotSet,
                        invoice_id: Set(value.id.clone()),
                        position: Set(position as i32),
                        name: Set(item.name.clone()),
                        specification: Set(item.specification.clone()),
                        unit: Set(item.unit.clone()),
                        quantity: Set(item.quantity.clone()),
                        unit_price_fen: Set(item.unit_price_fen),
                        amount_fen: Set(item.amount_fen),
                        tax_rate: Set(item.tax_rate.clone()),
                        tax_fen: Set(item.tax_fen),
                        raw_json: Set(item.raw_json.clone()),
                    },
                ))
                .exec(&transaction)
                .await?;
            }
            invoice_source_link::ActiveModel {
                id: NotSet,
                invoice_id: Set(value.id.clone()),
                task_id: Set(task_id.to_owned()),
                page_start: Set(value.page_start),
                page_end: Set(value.page_end),
            }
            .insert(&transaction)
            .await?;
        }

        if is_recovery {
            if !matched_recoveries.is_empty() {
                invoice_recovery_marker::Entity::delete_many()
                    .filter(
                        invoice_recovery_marker::Column::SourceSha256
                            .eq(&recovery_markers[0].source_sha256),
                    )
                    .filter(
                        invoice_recovery_marker::Column::RecoveryIdentity
                            .is_in(matched_recoveries.iter().cloned()),
                    )
                    .exec(&transaction)
                    .await?;
            }
            invoice_recovery_marker::Entity::update_many()
                .col_expr(
                    invoice_recovery_marker::Column::RecoveryTaskId,
                    sea_orm::sea_query::Expr::value(Option::<String>::None),
                )
                .filter(invoice_recovery_marker::Column::RecoveryTaskId.eq(task_id))
                .exec(&transaction)
                .await?;
            if inserted_ids.is_empty() {
                transaction.commit().await?;
                return Ok(PersistInvoicesOutcome::RecoveryNoMatch {
                    existing_invoice_id: first_existing,
                });
            }
        }

        let unmatched_recoveries = recovery_markers.len() - matched_recoveries.len();
        let warning = recovery_warning(
            warning,
            is_recovery.then_some((inserted_ids.len(), skipped_existing, unmatched_recoveries)),
        );
        let result = transaction.execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE invoice_tasks SET status = 'succeeded', error_code = NULL, error_message = NULL, warning_message = ?, request_id = ?, raw_response_json = ?, completed_at = ? WHERE id = ? AND status IN ('processing', 'awaiting_review')",
            [warning.into(), request_id.into(), raw_response.into(), timestamp.into(), task_id.into()],
        )).await?;
        if result.rows_affected() != 1 {
            return Err(AppError::InvalidInput("发票任务状态已变化".into()));
        }
        transaction.commit().await?;
        let task = self.invoice_task(task_id).await?;
        self.refresh_invoice_batch_status(&task.batch_id).await?;
        Ok(PersistInvoicesOutcome::Inserted {
            invoice_ids: inserted_ids,
        })
    }

    pub async fn invoice_batch(&self, id: &str) -> AppResult<InvoiceBatchDetail> {
        let batch = invoice_batch::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("发票批次 {id}")))?;
        let tasks = batch
            .find_related(invoice_task::Entity)
            .order_by_asc(invoice_task::Column::CreatedAt)
            .order_by_asc(invoice_task::Column::Id)
            .all(&self.connection)
            .await?
            .into_iter()
            .map(InvoiceTask::from)
            .collect::<Vec<_>>();
        Ok(InvoiceBatchDetail {
            id: batch.id,
            status: batch.status,
            forced_type: batch.forced_type,
            created_at: batch.created_at,
            completed_at: batch.completed_at,
            counts: invoice_counts(&tasks),
            tasks,
        })
    }

    pub async fn latest_invoice_batch(&self) -> AppResult<Option<InvoiceBatchDetail>> {
        let id = invoice_batch::Entity::find()
            .select_only()
            .column(invoice_batch::Column::Id)
            .order_by_desc(invoice_batch::Column::CreatedAt)
            .order_by_desc(invoice_batch::Column::Id)
            .into_tuple::<String>()
            .one(&self.connection)
            .await?;
        match id {
            Some(id) => self.invoice_batch(&id).await.map(Some),
            None => Ok(None),
        }
    }

    pub async fn list_invoice_batches(
        &self,
        query: InvoiceBatchesQuery,
    ) -> AppResult<InvoiceBatchesPage> {
        let page = query.page.unwrap_or(1).max(1);
        let page_size = query.page_size.unwrap_or(20).clamp(1, 100);
        let total = invoice_batch::Entity::find()
            .count(&self.connection)
            .await? as i64;
        let rows = InvoiceBatchSummaryRow::find_by_statement(Statement::from_sql_and_values(DbBackend::Sqlite,
            "SELECT b.id,b.status,b.forced_type,b.created_at,b.completed_at,COUNT(t.id) total,COALESCE(SUM(t.status='queued'),0) queued,COALESCE(SUM(t.status='processing'),0) processing,COALESCE(SUM(t.status='awaiting_review'),0) awaiting_review,COALESCE(SUM(t.status='succeeded'),0) succeeded,COALESCE(SUM(t.status='failed'),0) failed,COALESCE(SUM(t.status='canceled'),0) canceled,COALESCE(SUM(t.status='interrupted'),0) interrupted FROM invoice_batches b LEFT JOIN invoice_tasks t ON t.batch_id=b.id GROUP BY b.id ORDER BY b.created_at DESC,b.id DESC LIMIT ? OFFSET ?",
            [i64::from(page_size).into(), (i64::from(page-1)*i64::from(page_size)).into()])).all(&self.connection).await?;
        Ok(InvoiceBatchesPage {
            items: rows.into_iter().map(Into::into).collect(),
            total,
            page,
            page_size,
        })
    }

    pub async fn list_invoices(&self, query: InvoicesQuery) -> AppResult<InvoicesPage> {
        let page = query.page.unwrap_or(1).max(1);
        let page_size = query.page_size.unwrap_or(20).clamp(1, 100);
        let mut filter = String::new();
        let mut values = Vec::<sea_orm::Value>::new();
        if let Some(search) = query
            .search
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
        {
            filter.push_str(" AND (i.invoice_code LIKE ? OR i.invoice_number LIKE ? OR i.buyer_name LIKE ? OR i.buyer_tax_id LIKE ? OR i.seller_name LIKE ? OR i.seller_tax_id LIKE ?)");
            let p = format!("%{search}%");
            for _ in 0..6 {
                values.push(p.clone().into());
            }
        }
        if let Some(subtype) = query.subtype.filter(|v| !v.trim().is_empty()) {
            filter.push_str(" AND i.subtype = ?");
            values.push(subtype.into());
        }
        if let Some(from) = query.date_from.filter(|v| !v.trim().is_empty()) {
            filter.push_str(" AND i.invoice_date >= ?");
            values.push(from.into());
        }
        if let Some(to) = query.date_to.filter(|v| !v.trim().is_empty()) {
            filter.push_str(" AND i.invoice_date <= ?");
            values.push(to.into());
        }
        if query.duplicate_only.unwrap_or(false) {
            filter.push_str(" AND i.possible_duplicate = 1");
        }
        if let Some(project_id) = query.project_id.filter(|value| !value.trim().is_empty()) {
            filter.push_str(" AND pi.project_id = ?");
            values.push(project_id.into());
        }
        if query.unassigned_only.unwrap_or(false) {
            filter.push_str(" AND pi.invoice_id IS NULL");
        }
        #[derive(FromQueryResult)]
        struct CountRow {
            count: i64,
        }
        let total = CountRow::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            format!("SELECT COUNT(*) count FROM invoices i LEFT JOIN project_invoices pi ON pi.invoice_id=i.id WHERE 1=1{filter}"),
            values.clone(),
        ))
        .one(&self.connection)
        .await?
        .map(|r| r.count)
        .unwrap_or(0);
        let column = match query.sort_by.unwrap_or(InvoiceSort::CreatedAt) {
            InvoiceSort::InvoiceDate => "i.invoice_date",
            InvoiceSort::TotalAmount => "i.total_fen",
            InvoiceSort::CreatedAt => "i.created_at",
        };
        let direction = match query.sort_direction.unwrap_or(SortDirection::Desc) {
            SortDirection::Asc => "ASC",
            SortDirection::Desc => "DESC",
        };
        values.extend([
            i64::from(page_size).into(),
            (i64::from(page - 1) * i64::from(page_size)).into(),
        ]);
        let items=InvoiceListItem::find_by_statement(Statement::from_sql_and_values(DbBackend::Sqlite,format!("SELECT i.id,i.type type_id,i.subtype,i.subtype_description,i.invoice_code,i.invoice_number,i.invoice_date,i.buyer_name,i.seller_name,i.total_fen,i.currency,i.possible_duplicate,i.created_at,pi.project_id,p.name project_name FROM invoices i LEFT JOIN project_invoices pi ON pi.invoice_id=i.id LEFT JOIN projects p ON p.id=pi.project_id WHERE 1=1{filter} ORDER BY {column} {direction},i.id DESC LIMIT ? OFFSET ?"),values)).all(&self.connection).await?;
        Ok(InvoicesPage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub async fn invoice_detail(&self, id: &str) -> AppResult<InvoiceDetail> {
        let value = invoice::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("发票 {id}")))?;
        let task = invoice_task::Entity::find_by_id(&value.task_id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("发票源文件 {id}")))?;
        let items = value
            .find_related(invoice_line_item::Entity)
            .order_by_asc(invoice_line_item::Column::Position)
            .all(&self.connection)
            .await?
            .into_iter()
            .map(InvoiceLineItem::from)
            .collect();
        let is_matched = invoice_match::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .is_some();
        Ok(InvoiceDetail {
            id: value.id,
            task_id: value.task_id,
            type_id: value.r#type,
            subtype: value.subtype,
            type_description: value.type_description,
            subtype_description: value.subtype_description,
            invoice_code: value.invoice_code,
            invoice_number: value.invoice_number,
            invoice_date: value.invoice_date,
            buyer_name: value.buyer_name,
            buyer_tax_id: value.buyer_tax_id,
            seller_name: value.seller_name,
            seller_tax_id: value.seller_tax_id,
            amount_fen: value.amount_fen,
            tax_fen: value.tax_fen,
            total_fen: value.total_fen,
            currency: value.currency,
            possible_duplicate: value.possible_duplicate,
            preview_path: value.preview_path,
            source_path: task.source_path.unwrap_or_default(),
            original_file_name: task.original_file_name,
            page_start: value.page_start,
            page_end: value.page_end,
            raw_invoice_json: value.raw_invoice_json,
            raw_response_json: task.raw_response_json.unwrap_or_default(),
            request_id: task.request_id,
            created_at: value.created_at,
            updated_at: value.updated_at,
            items,
            is_matched,
        })
    }

    pub async fn update_invoice(&self, request: UpdateInvoiceRequest) -> AppResult<InvoiceDetail> {
        if request.amount_fen.is_some_and(|v| v < 0)
            || request.tax_fen.is_some_and(|v| v < 0)
            || request.total_fen.is_some_and(|v| v < 0)
        {
            return Err(AppError::InvalidInput("发票金额不能为负数".into()));
        }
        let transaction = self.connection.begin().await?;
        let model = invoice::Entity::find_by_id(&request.id)
            .one(&transaction)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("发票 {}", request.id)))?;
        let code = clean(request.invoice_code);
        let number = clean(request.invoice_number);
        let identity = reliable_identity(&model.subtype, code.as_deref(), number.as_deref());
        let has_reliable_identity = identity.is_some();
        if let Some(ref key) = identity
            && invoice::Entity::find()
                .filter(invoice::Column::ReliableIdentity.eq(key))
                .filter(invoice::Column::Id.ne(&request.id))
                .one(&transaction)
                .await?
                .is_some()
        {
            return Err(AppError::InvalidInput(
                "发票代码和号码与现有记录重复".into(),
            ));
        }
        let old_subtype = model.subtype.clone();
        let old_number = model.invoice_number.clone();
        let new_subtype = model.subtype.clone();
        let next_invoice_date = clean(request.invoice_date);
        let next_currency = request.currency.trim().to_uppercase();
        let matching_fields_changed = model.invoice_date != next_invoice_date
            || model.total_fen != request.total_fen
            || model.currency != next_currency;
        if matching_fields_changed {
            invoice_match::Entity::delete_by_id(&request.id)
                .exec(&transaction)
                .await?;
        }
        let mut active = model.into_active_model();
        active.invoice_code = Set(code);
        active.invoice_number = Set(number.clone());
        active.invoice_date = Set(next_invoice_date);
        active.buyer_name = Set(clean(request.buyer_name));
        active.buyer_tax_id = Set(clean(request.buyer_tax_id));
        active.seller_name = Set(clean(request.seller_name));
        active.seller_tax_id = Set(clean(request.seller_tax_id));
        active.amount_fen = Set(request.amount_fen);
        active.tax_fen = Set(request.tax_fen);
        active.total_fen = Set(request.total_fen);
        active.currency = Set(next_currency);
        active.reliable_identity = Set(identity);
        if has_reliable_identity || number.is_none() {
            active.possible_duplicate = Set(false);
        }
        active.updated_at = Set(now());
        active.update(&transaction).await?;
        refresh_weak_duplicate_group(&transaction, &old_subtype, old_number.as_deref()).await?;
        refresh_weak_duplicate_group(&transaction, &new_subtype, number.as_deref()).await?;
        transaction.commit().await?;
        self.invoice_detail(&request.id).await
    }

    pub async fn invoice_paths_for_delete(
        &self,
        id: &str,
    ) -> AppResult<(String, String, String, bool)> {
        if invoice_match::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .is_some()
        {
            return Err(AppError::MatchingLocked(format!("发票 {id}")));
        }
        let detail = self.invoice_detail(id).await?;
        let count = invoice::Entity::find()
            .filter(invoice::Column::TaskId.eq(&detail.task_id))
            .count(&self.connection)
            .await?;
        Ok((
            detail.task_id,
            detail.preview_path,
            detail.source_path,
            count == 1,
        ))
    }

    pub async fn delete_invoice_record(&self, id: &str) -> AppResult<bool> {
        let transaction = self.connection.begin().await?;
        let value = invoice::Entity::find_by_id(id)
            .one(&transaction)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("发票 {id}")))?;
        let task_id = value.task_id.clone();
        let recovery_identity =
            invoice_recovery_identity(&value.subtype, value.invoice_number.as_deref());
        let task = invoice_task::Entity::find_by_id(&task_id)
            .one(&transaction)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("发票任务 {task_id}")))?;
        let old_subtype = value.subtype.clone();
        let old_number = value.invoice_number.clone();
        let project_id = project_invoice::Entity::find()
            .filter(project_invoice::Column::InvoiceId.eq(id))
            .one(&transaction)
            .await?
            .map(|membership| membership.project_id);
        if let Some(recovery_identity) = recovery_identity {
            invoice_recovery_marker::ActiveModel {
                source_sha256: Set(task.source_sha256),
                recovery_identity: Set(recovery_identity),
                recovery_task_id: Set(None),
                deleted_at: Set(now()),
            }
            .insert(&transaction)
            .await?;
        }
        invoice::Entity::delete_by_id(id).exec(&transaction).await?;
        if let Some(project_id) = project_id {
            compact_project(&transaction, &project_id).await?;
            touch_project(&transaction, &project_id).await?;
        }
        refresh_weak_duplicate_group(&transaction, &old_subtype, old_number.as_deref()).await?;
        let remaining = invoice::Entity::find()
            .filter(invoice::Column::TaskId.eq(&task_id))
            .count(&transaction)
            .await?;
        if remaining == 0 {
            invoice_task::Entity::delete_by_id(&task_id)
                .exec(&transaction)
                .await?;
        }
        transaction.commit().await?;
        Ok(remaining == 0)
    }

    pub async fn invoice_task_raw_response(
        &self,
        id: &str,
    ) -> AppResult<(InvoiceTask, String, String)> {
        let model = invoice_task::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("发票任务 {id}")))?;
        let raw = model
            .raw_response_json
            .clone()
            .ok_or_else(|| AppError::InvalidInput("该任务没有可复核的识别结果".into()))?;
        let request_id = model.request_id.clone().unwrap_or_default();
        Ok((InvoiceTask::from(model), request_id, raw))
    }

    pub async fn clear_invoice_source_path(&self, id: &str) -> AppResult<()> {
        invoice_task::Entity::update_many()
            .col_expr(
                invoice_task::Column::SourcePath,
                sea_orm::sea_query::Expr::value(Option::<String>::None),
            )
            .filter(invoice_task::Column::Id.eq(id))
            .exec(&self.connection)
            .await?;
        Ok(())
    }

    async fn set_invoice_batch_processing(&self, id: &str) -> AppResult<()> {
        invoice_batch::Entity::update_many()
            .col_expr(
                invoice_batch::Column::Status,
                sea_orm::sea_query::Expr::value("processing"),
            )
            .col_expr(
                invoice_batch::Column::CompletedAt,
                sea_orm::sea_query::Expr::value(Option::<String>::None),
            )
            .filter(invoice_batch::Column::Id.eq(id))
            .exec(&self.connection)
            .await?;
        Ok(())
    }
    pub async fn finalize_invoice_batch(&self, id: &str) -> AppResult<()> {
        self.refresh_invoice_batch_status(id).await
    }
    pub(crate) async fn refresh_open_invoice_batches(&self) -> AppResult<()> {
        let ids = invoice_batch::Entity::find()
            .select_only()
            .column(invoice_batch::Column::Id)
            .filter(invoice_batch::Column::Status.is_in(["queued", "processing"]))
            .into_tuple::<String>()
            .all(&self.connection)
            .await?;
        for id in ids {
            self.refresh_invoice_batch_status(&id).await?;
        }
        Ok(())
    }
    async fn refresh_invoice_batch_status(&self, id: &str) -> AppResult<()> {
        let tasks = invoice_task::Entity::find()
            .filter(invoice_task::Column::BatchId.eq(id))
            .all(&self.connection)
            .await?
            .into_iter()
            .map(InvoiceTask::from)
            .collect::<Vec<_>>();
        let counts = invoice_counts(&tasks);
        let (status, done) = if counts.queued + counts.processing > 0 {
            ("processing", None)
        } else if counts.awaiting_review > 0 {
            ("awaiting_review", None)
        } else if counts.failed + counts.canceled + counts.interrupted > 0 {
            ("completed_with_errors", Some(now()))
        } else {
            ("completed", Some(now()))
        };
        let model = invoice_batch::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("发票批次 {id}")))?;
        let mut active = model.into_active_model();
        active.status = Set(status.into());
        active.completed_at = Set(done);
        active.update(&self.connection).await?;
        Ok(())
    }
}

#[derive(FromQueryResult)]
struct InvoiceBatchSummaryRow {
    id: String,
    status: String,
    forced_type: Option<i32>,
    created_at: String,
    completed_at: Option<String>,
    total: i64,
    queued: i64,
    processing: i64,
    awaiting_review: i64,
    succeeded: i64,
    failed: i64,
    canceled: i64,
    interrupted: i64,
}
impl From<InvoiceBatchSummaryRow> for InvoiceBatchSummary {
    fn from(v: InvoiceBatchSummaryRow) -> Self {
        Self {
            id: v.id,
            status: v.status,
            forced_type: v.forced_type,
            created_at: v.created_at,
            completed_at: v.completed_at,
            counts: InvoiceBatchCounts {
                total: v.total,
                queued: v.queued,
                processing: v.processing,
                awaiting_review: v.awaiting_review,
                succeeded: v.succeeded,
                failed: v.failed,
                canceled: v.canceled,
                interrupted: v.interrupted,
            },
        }
    }
}
impl From<invoice_task::Model> for InvoiceTask {
    fn from(v: invoice_task::Model) -> Self {
        let review_items = if v.status == "awaiting_review" {
            review_items(v.raw_response_json.as_deref())
        } else {
            Vec::new()
        };
        Self {
            id: v.id,
            batch_id: v.batch_id,
            original_file_name: v.original_file_name,
            source_path: v.source_path,
            source_sha256: v.source_sha256,
            media_type: v.media_type,
            forced_type: v.forced_type,
            status: v.status,
            error_code: v.error_code,
            error_message: v.error_message,
            warning_message: v.warning_message,
            retry_count: i64::from(v.retry_count),
            request_id: v.request_id,
            existing_invoice_id: v.existing_invoice_id,
            review_items,
            created_at: v.created_at,
            started_at: v.started_at,
            completed_at: v.completed_at,
        }
    }
}
impl From<invoice_line_item::Model> for InvoiceLineItem {
    fn from(v: invoice_line_item::Model) -> Self {
        Self {
            id: v.id,
            position: v.position,
            name: v.name,
            specification: v.specification,
            unit: v.unit,
            quantity: v.quantity,
            unit_price_fen: v.unit_price_fen,
            amount_fen: v.amount_fen,
            tax_rate: v.tax_rate,
            tax_fen: v.tax_fen,
        }
    }
}
fn invoice_counts(tasks: &[InvoiceTask]) -> InvoiceBatchCounts {
    let mut c = InvoiceBatchCounts {
        total: tasks.len() as i64,
        ..Default::default()
    };
    for t in tasks {
        match t.status.as_str() {
            "queued" => c.queued += 1,
            "processing" => c.processing += 1,
            "awaiting_review" => c.awaiting_review += 1,
            "succeeded" => c.succeeded += 1,
            "failed" => c.failed += 1,
            "canceled" => c.canceled += 1,
            "interrupted" => c.interrupted += 1,
            _ => {}
        }
    }
    c
}
pub fn reliable_identity(
    subtype: &str,
    code: Option<&str>,
    number: Option<&str>,
) -> Option<String> {
    let number = number.map(str::trim).filter(|v| !v.is_empty())?;
    let code = code.map(str::trim).filter(|v| !v.is_empty());
    if code.is_none() && !subtype.ends_with("Full") {
        return None;
    }
    Some(format!(
        "{}|{}|{}",
        subtype.to_ascii_lowercase(),
        code.unwrap_or_default().to_ascii_uppercase(),
        number.to_ascii_uppercase()
    ))
}
fn invoice_recovery_identity(subtype: &str, number: Option<&str>) -> Option<String> {
    let number = number.map(str::trim).filter(|value| !value.is_empty())?;
    Some(format!(
        "{}|{}",
        subtype.trim().to_ascii_lowercase(),
        number.to_ascii_uppercase()
    ))
}
fn clean(value: Option<String>) -> Option<String> {
    value.and_then(|v| {
        let v = v.trim().to_owned();
        (!v.is_empty()).then_some(v)
    })
}
fn recovery_warning(
    original: Option<&str>,
    recovery: Option<(usize, usize, usize)>,
) -> Option<String> {
    let mut parts = original
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .into_iter()
        .collect::<Vec<_>>();
    if let Some((inserted, skipped, unmatched)) = recovery {
        parts.push(format!(
            "补齐 {inserted} 张已删除发票，跳过 {skipped} 张已存在发票"
        ));
        if unmatched > 0 {
            parts.push(format!("{unmatched} 张待补齐发票未在本次识别结果中出现"));
        }
    }
    (!parts.is_empty()).then(|| parts.join("；"))
}
async fn refresh_weak_duplicate_group(
    transaction: &sea_orm::DatabaseTransaction,
    subtype: &str,
    number: Option<&str>,
) -> AppResult<()> {
    let Some(number) = number.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    transaction.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "UPDATE invoices SET possible_duplicate = CASE WHEN (SELECT COUNT(*) FROM invoices grouped WHERE grouped.subtype = ? AND grouped.invoice_number = ? AND grouped.reliable_identity IS NULL) > 1 THEN 1 ELSE 0 END WHERE subtype = ? AND invoice_number = ? AND reliable_identity IS NULL",
        [subtype.into(), number.into(), subtype.into(), number.into()],
    )).await?;
    Ok(())
}
fn review_items(raw: Option<&str>) -> Vec<crate::models::InvoiceReviewItem> {
    let Some(raw) = raw else { return Vec::new() };
    let Ok(parsed) = crate::tencent_ocr::parse_response(raw.to_owned()) else {
        return Vec::new();
    };
    parsed
        .invoices
        .into_iter()
        .enumerate()
        .map(|(index, item)| crate::models::InvoiceReviewItem {
            index,
            page: item.page_start,
            subtype: item.subtype,
            subtype_description: item.subtype_description,
            invoice_number: item.invoice_number,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{InvoiceSort, NormalizedInvoice, SortDirection};

    fn normalized(id: &str, code: Option<&str>, number: &str) -> NormalizedInvoice {
        NormalizedInvoice {
            id: id.into(),
            type_id: 3,
            subtype: "VatSpecialInvoice".into(),
            type_description: "增值税发票".into(),
            subtype_description: "增值税专用发票".into(),
            invoice_code: code.map(str::to_owned),
            invoice_number: Some(number.into()),
            invoice_date: Some("2026-08-12".into()),
            buyer_name: Some("购买方".into()),
            buyer_tax_id: None,
            seller_name: Some("销售方".into()),
            seller_tax_id: None,
            amount_fen: Some(1_000),
            tax_fen: Some(60),
            total_fen: Some(1_060),
            currency: "CNY".into(),
            reliable_identity: reliable_identity("VatSpecialInvoice", code, Some(number)),
            possible_duplicate: false,
            preview_path: format!("invoices/previews/{id}.png"),
            page_start: 1,
            page_end: 1,
            raw_invoice_json: "{}".into(),
            items: Vec::new(),
        }
    }

    async fn processing_task(db: &Database, batch: &str, task: &str, hash: &str) {
        db.create_invoice_batch(batch, None).await.unwrap();
        db.create_invoice_task(
            task,
            batch,
            "invoice.png",
            Some(&format!("invoices/sources/{task}.png")),
            hash,
            "image/png",
            None,
            "queued",
            None,
            None,
        )
        .await
        .unwrap();
        assert!(db.mark_invoice_processing(task).await.unwrap());
    }

    #[tokio::test]
    async fn reliable_identity_collision_is_a_hard_duplicate() {
        let db = Database::in_memory().await.unwrap();
        processing_task(&db, "batch-a", "task-a", "hash-a").await;
        let first = normalized("invoice-a", Some("4403"), "10001");
        assert!(matches!(
            db.persist_invoices(
                "task-a",
                "request-a",
                "{}",
                &[first],
                Some("一个识别项失败"),
            )
            .await
            .unwrap(),
            PersistInvoicesOutcome::Inserted { .. }
        ));
        assert_eq!(
            db.invoice_task("task-a")
                .await
                .unwrap()
                .warning_message
                .as_deref(),
            Some("一个识别项失败")
        );

        processing_task(&db, "batch-b", "task-b", "hash-b").await;
        let second = normalized("invoice-b", Some("4403"), "10001");
        let duplicate = db
            .persist_invoices("task-b", "request-b", "{}", &[second], None)
            .await
            .unwrap();
        assert!(matches!(
            duplicate,
            PersistInvoicesOutcome::Duplicate {
                existing_invoice_id
            } if existing_invoice_id == "invoice-a"
        ));
        let page = db
            .list_invoices(InvoicesQuery {
                page: None,
                page_size: None,
                search: None,
                subtype: None,
                date_from: None,
                date_to: None,
                duplicate_only: None,
                project_id: None,
                unassigned_only: None,
                sort_by: Some(InvoiceSort::CreatedAt),
                sort_direction: Some(SortDirection::Desc),
            })
            .await
            .unwrap();
        assert_eq!(page.total, 1);
    }

    #[tokio::test]
    async fn source_less_audit_task_does_not_claim_invoice_hash() {
        let db = Database::in_memory().await.unwrap();
        db.create_invoice_batch("audit-batch", None).await.unwrap();
        db.create_invoice_task(
            "audit-task",
            "audit-batch",
            "invoice.png",
            None,
            "audit-hash",
            "image/png",
            None,
            "failed",
            Some("duplicate_file"),
            Some("duplicate"),
        )
        .await
        .unwrap();

        assert!(!db.invoice_hash_exists("audit-hash").await.unwrap());
    }

    #[tokio::test]
    async fn create_time_duplicate_check_leaves_no_partial_source_task() {
        let db = Database::in_memory().await.unwrap();
        processing_task(&db, "batch-a", "task-a", "claimed-hash").await;
        db.create_invoice_batch("batch-b", None).await.unwrap();

        assert!(
            !db.create_invoice_source_task(
                "task-b",
                "batch-b",
                "invoice.png",
                "invoices/sources/task-b.png",
                "claimed-hash",
                "image/png",
                None,
            )
            .await
            .unwrap()
        );
        assert!(matches!(
            db.invoice_task("task-b").await,
            Err(AppError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn recovery_import_restores_only_deleted_invoice_from_multi_invoice_source() {
        let db = Database::in_memory().await.unwrap();
        processing_task(&db, "batch-a", "task-a", "shared-hash").await;
        db.persist_invoices(
            "task-a",
            "request-a",
            "{}",
            &[
                normalized("invoice-a", Some("4403"), "10001"),
                normalized("invoice-b", Some("4403"), "10002"),
            ],
            None,
        )
        .await
        .unwrap();
        assert!(db.invoice_hash_exists("shared-hash").await.unwrap());

        assert!(!db.delete_invoice_record("invoice-a").await.unwrap());
        assert!(!db.invoice_hash_exists("shared-hash").await.unwrap());
        assert!(db.invoice_detail("invoice-b").await.is_ok());

        db.create_invoice_batch("batch-recovery", None)
            .await
            .unwrap();
        assert!(
            db.create_invoice_source_task(
                "task-recovery",
                "batch-recovery",
                "invoice.png",
                "invoices/sources/task-recovery.png",
                "shared-hash",
                "image/png",
                None,
            )
            .await
            .unwrap()
        );
        assert!(db.mark_invoice_processing("task-recovery").await.unwrap());
        let outcome = db
            .persist_invoices(
                "task-recovery",
                "request-recovery",
                "{}",
                &[
                    normalized("invoice-a-restored", Some("4403"), "10001"),
                    normalized("invoice-b-duplicate", Some("4403"), "10002"),
                    normalized("invoice-new", Some("4403"), "10003"),
                ],
                None,
            )
            .await
            .unwrap();

        assert!(matches!(
            outcome,
            PersistInvoicesOutcome::Inserted { invoice_ids }
                if invoice_ids == HashSet::from(["invoice-a-restored".to_owned()])
        ));
        assert!(db.invoice_detail("invoice-a-restored").await.is_ok());
        assert!(db.invoice_detail("invoice-b").await.is_ok());
        assert!(db.invoice_detail("invoice-b-duplicate").await.is_err());
        assert!(db.invoice_detail("invoice-new").await.is_err());
        assert!(db.invoice_hash_exists("shared-hash").await.unwrap());
        assert!(
            invoice_recovery_marker::Entity::find()
                .all(&db.connection)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn recovery_no_match_releases_marker_for_another_attempt() {
        let db = Database::in_memory().await.unwrap();
        processing_task(&db, "batch-a", "task-a", "shared-hash").await;
        db.persist_invoices(
            "task-a",
            "request-a",
            "{}",
            &[normalized("invoice-a", Some("4403"), "10001")],
            None,
        )
        .await
        .unwrap();
        assert!(db.delete_invoice_record("invoice-a").await.unwrap());

        db.create_invoice_batch("batch-recovery", None)
            .await
            .unwrap();
        assert!(
            db.create_invoice_source_task(
                "task-recovery",
                "batch-recovery",
                "invoice.png",
                "invoices/sources/task-recovery.png",
                "shared-hash",
                "image/png",
                None,
            )
            .await
            .unwrap()
        );
        assert!(db.mark_invoice_processing("task-recovery").await.unwrap());
        let outcome = db
            .persist_invoices(
                "task-recovery",
                "request-recovery",
                "{}",
                &[normalized("other-invoice", Some("4403"), "99999")],
                None,
            )
            .await
            .unwrap();

        assert!(matches!(
            outcome,
            PersistInvoicesOutcome::RecoveryNoMatch {
                existing_invoice_id: None
            }
        ));
        let marker = invoice_recovery_marker::Entity::find()
            .one(&db.connection)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(marker.source_sha256, "shared-hash");
        assert!(marker.recovery_task_id.is_none());
        assert!(!db.invoice_hash_exists("shared-hash").await.unwrap());

        db.create_invoice_batch("batch-retry", None).await.unwrap();
        assert!(
            db.create_invoice_source_task(
                "task-retry",
                "batch-retry",
                "invoice.png",
                "invoices/sources/task-retry.png",
                "shared-hash",
                "image/png",
                None,
            )
            .await
            .unwrap()
        );
        assert!(db.mark_invoice_processing("task-retry").await.unwrap());
        let outcome = db
            .persist_invoices(
                "task-retry",
                "request-retry",
                "{}",
                &[normalized("invoice-restored", Some("4403"), "10001")],
                None,
            )
            .await
            .unwrap();
        assert!(matches!(
            outcome,
            PersistInvoicesOutcome::Inserted { invoice_ids }
                if invoice_ids == HashSet::from(["invoice-restored".to_owned()])
        ));
        assert!(db.invoice_detail("invoice-restored").await.is_ok());
        assert!(
            invoice_recovery_marker::Entity::find()
                .all(&db.connection)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(db.invoice_hash_exists("shared-hash").await.unwrap());
    }

    #[tokio::test]
    async fn weak_number_collision_marks_possible_duplicates() {
        let db = Database::in_memory().await.unwrap();
        processing_task(&db, "batch-a", "task-a", "hash-a").await;
        let first = normalized("invoice-a", None, "10001");
        db.persist_invoices("task-a", "request-a", "{}", &[first], None)
            .await
            .unwrap();
        processing_task(&db, "batch-b", "task-b", "hash-b").await;
        let second = normalized("invoice-b", None, "10001");
        db.persist_invoices("task-b", "request-b", "{}", &[second], None)
            .await
            .unwrap();
        let page = db
            .list_invoices(InvoicesQuery {
                page: None,
                page_size: None,
                search: None,
                subtype: None,
                date_from: None,
                date_to: None,
                duplicate_only: Some(true),
                project_id: None,
                unassigned_only: None,
                sort_by: None,
                sort_direction: None,
            })
            .await
            .unwrap();
        assert_eq!(page.total, 2);
        assert!(page.items.iter().all(|invoice| invoice.possible_duplicate));
    }

    #[tokio::test]
    async fn clearing_a_weak_number_recalculates_both_records() {
        let db = Database::in_memory().await.unwrap();
        processing_task(&db, "batch-a", "task-a", "hash-a").await;
        db.persist_invoices(
            "task-a",
            "request-a",
            "{}",
            &[normalized("invoice-a", None, "10001")],
            None,
        )
        .await
        .unwrap();
        processing_task(&db, "batch-b", "task-b", "hash-b").await;
        db.persist_invoices(
            "task-b",
            "request-b",
            "{}",
            &[normalized("invoice-b", None, "10001")],
            None,
        )
        .await
        .unwrap();
        let original = db.invoice_detail("invoice-b").await.unwrap();
        db.update_invoice(UpdateInvoiceRequest {
            id: original.id,
            invoice_code: None,
            invoice_number: None,
            invoice_date: original.invoice_date,
            buyer_name: original.buyer_name,
            buyer_tax_id: original.buyer_tax_id,
            seller_name: original.seller_name,
            seller_tax_id: original.seller_tax_id,
            amount_fen: original.amount_fen,
            tax_fen: original.tax_fen,
            total_fen: original.total_fen,
            currency: original.currency,
        })
        .await
        .unwrap();

        assert!(
            !db.invoice_detail("invoice-a")
                .await
                .unwrap()
                .possible_duplicate
        );
        assert!(
            !db.invoice_detail("invoice-b")
                .await
                .unwrap()
                .possible_duplicate
        );
    }

    #[tokio::test]
    async fn failed_task_without_source_cannot_retry() {
        let db = Database::in_memory().await.unwrap();
        db.create_invoice_batch("batch", None).await.unwrap();
        db.create_invoice_task(
            "task",
            "batch",
            "bad.gif",
            None,
            "hash",
            "unknown",
            None,
            "failed",
            Some("invalid_file"),
            Some("不支持的文件"),
        )
        .await
        .unwrap();
        assert!(matches!(
            db.prepare_invoice_retry("task", None).await,
            Err(AppError::InvalidInput(_))
        ));
    }
}
