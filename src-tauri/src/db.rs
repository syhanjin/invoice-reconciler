use std::path::Path;
use std::time::Duration;

use chrono::Utc;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ActiveValue::NotSet, ColumnTrait, ConnectOptions, ConnectionTrait,
    DatabaseConnection, DatabaseTransaction, DbBackend, EntityTrait, FromQueryResult,
    IntoActiveModel, ModelTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set,
    Statement, TransactionTrait,
};
use sea_orm_migration::MigratorTrait;

use crate::entities::{
    app_setting, import_batch, import_task, invoice_match, invoice_match_order, order, order_item,
};
use crate::error::{AppError, AppResult};
use crate::migration::Migrator;
use crate::models::{
    AppSettings, DEFAULT_MAX_CONCURRENCY, OrderDetail, OrderListItem, OrderSort, OrdersPage,
    OrdersQuery, SortDirection, UpdateOrderRequest,
};
#[cfg(test)]
use crate::models::{
    BatchCounts, ImportBatchSummary, ImportBatchesPage, ImportBatchesQuery, ImportTask,
    NormalizedOrder, Platform,
};

#[derive(Clone)]
pub struct Database {
    pub(crate) connection: DatabaseConnection,
}

const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_secs(5);

fn sqlite_connect_options(url: String) -> ConnectOptions {
    let mut options = ConnectOptions::new(url);
    options
        .max_connections(1)
        .sqlx_logging(false)
        .map_sqlx_sqlite_opts(|options| {
            options.foreign_keys(true).busy_timeout(SQLITE_BUSY_TIMEOUT)
        });
    options
}

pub(crate) fn normalized_ledger_filter(value: Option<&str>) -> AppResult<Option<&str>> {
    match value
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "all")
    {
        None => Ok(None),
        Some(value @ ("unmatched" | "matched" | "confirmed")) => Ok(Some(value)),
        Some(_) => Err(AppError::InvalidInput("匹配状态筛选无效".into())),
    }
}

pub(crate) fn normalized_project_filter(value: Option<&str>) -> AppResult<Option<&str>> {
    match value
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "all")
    {
        None => Ok(None),
        Some(value @ ("assigned" | "unassigned")) => Ok(Some(value)),
        Some(_) => Err(AppError::InvalidInput("项目状态筛选无效".into())),
    }
}

impl Database {
    pub async fn connect(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let options = sqlite_connect_options(format!("sqlite://{}?mode=rwc", path.display()));
        let connection = sea_orm::Database::connect(options).await?;
        Migrator::up(&connection, None).await?;
        connection
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "UPDATE invoice_tasks SET status = 'interrupted', error_code = 'app_interrupted', \
                 error_message = '应用在识别完成前退出，可手动重试', completed_at = ? \
                 WHERE status IN ('queued', 'processing')",
                [now().into()],
            ))
            .await?;
        connection
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "UPDATE import_tasks SET status = 'interrupted', error_code = 'app_interrupted', \
                 error_message = '应用在识别完成前退出，可手动重试', completed_at = ? \
                 WHERE status IN ('queued', 'processing')",
                [now().into()],
            ))
            .await?;
        connection
            .execute(Statement::from_string(
                DbBackend::Sqlite,
                "UPDATE order_import_pool SET status = 'failed', \
                 error_code = 'app_interrupted', error_message = '应用在实时识别完成前退出，可手动重试' \
                 WHERE status = 'processing'",
            ))
            .await?;
        connection
            .execute(Statement::from_string(
                DbBackend::Sqlite,
                "UPDATE match_summary_attempts SET status = 'interrupted', error_message = '应用在摘要生成完成前退出', completed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE status = 'processing'",
            ))
            .await?;
        connection
            .execute(Statement::from_string(
                DbBackend::Sqlite,
                "UPDATE invoice_matches SET summary_status = 'pending', summary_trigger = COALESCE(summary_trigger, 'interrupted'), summary_error = NULL \
                 WHERE summary_status = 'processing'",
            ))
            .await?;
        connection
            .execute(Statement::from_string(
                DbBackend::Sqlite,
                "DELETE FROM match_summary_attempts WHERE datetime(started_at) < datetime('now', '-180 days')",
            ))
            .await?;
        let database = Self { connection };
        database.refresh_open_batches().await?;
        database.refresh_open_invoice_batches().await?;
        Ok(database)
    }

    #[cfg(test)]
    pub async fn in_memory() -> AppResult<Self> {
        let options = sqlite_connect_options("sqlite::memory:".to_owned());
        let connection = sea_orm::Database::connect(options).await?;
        Migrator::up(&connection, None).await?;
        Ok(Self { connection })
    }

    #[cfg(test)]
    pub async fn execute_test_statement(&self, sql: &str) -> AppResult<()> {
        self.connection
            .execute(Statement::from_string(DbBackend::Sqlite, sql))
            .await?;
        Ok(())
    }

    pub async fn settings(&self) -> AppResult<AppSettings> {
        let model = app_setting::Entity::find_by_id(1)
            .one(&self.connection)
            .await?
            .ok_or_else(|| AppError::Database("缺少应用设置记录".into()))?;
        Ok(settings_from_model(model))
    }

    pub async fn hash_exists(&self, hash: &str) -> AppResult<bool> {
        let row = self
            .connection
            .query_one(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "SELECT EXISTS(SELECT 1 FROM order_import_pool WHERE image_sha256 = ? UNION ALL SELECT 1 FROM orders WHERE image_sha256 = ? AND managed_image_path IS NOT NULL) AS claimed",
                [hash.into(), hash.into()],
            ))
            .await?;
        Ok(row
            .and_then(|row| row.try_get::<i64>("", "claimed").ok())
            .unwrap_or(0)
            != 0)
    }

    #[cfg(test)]
    pub async fn create_batch(&self, id: &str, platform: Platform) -> AppResult<()> {
        import_batch::ActiveModel {
            id: Set(id.to_owned()),
            platform: Set(platform.as_str().to_owned()),
            status: Set("queued".into()),
            created_at: Set(now()),
            completed_at: Set(None),
        }
        .insert(&self.connection)
        .await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    #[cfg(test)]
    pub async fn create_task(
        &self,
        id: &str,
        batch_id: &str,
        original_file_name: &str,
        managed_image_path: Option<&str>,
        hash: &str,
        status: &str,
        message: Option<&str>,
    ) -> AppResult<()> {
        import_task::ActiveModel {
            id: Set(id.to_owned()),
            batch_id: Set(batch_id.to_owned()),
            original_file_name: Set(original_file_name.to_owned()),
            managed_image_path: Set(managed_image_path.map(str::to_owned)),
            image_sha256: Set(hash.to_owned()),
            status: Set(status.to_owned()),
            error_code: Set(None),
            error_message: Set(None),
            warning_message: Set(message.map(str::to_owned)),
            retry_count: Set(0),
            created_at: Set(now()),
            started_at: Set(None),
            completed_at: Set((status == "skipped").then(now)),
        }
        .insert(&self.connection)
        .await?;
        Ok(())
    }

    #[cfg(test)]
    pub async fn task(&self, id: &str) -> AppResult<ImportTask> {
        import_task::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .map(ImportTask::from)
            .ok_or_else(|| AppError::NotFound(format!("导入任务 {id}")))
    }

    #[cfg(test)]
    pub async fn mark_processing(&self, id: &str) -> AppResult<bool> {
        let result = self
            .connection
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "UPDATE import_tasks SET status = 'processing', error_code = NULL, error_message = NULL, \
                 warning_message = NULL, started_at = ?, completed_at = NULL \
                 WHERE id = ? AND status = 'queued'",
                [now().into(), id.into()],
            ))
            .await?;
        Ok(result.rows_affected() == 1)
    }

    #[cfg(test)]
    pub async fn persist_recognized_order(
        &self,
        task_id: &str,
        platform: Platform,
        normalized: &NormalizedOrder,
    ) -> AppResult<()> {
        let transaction = self.connection.begin().await?;
        let timestamp = now();
        order::ActiveModel {
            id: Set(task_id.to_owned()),
            platform: Set(platform.as_str().to_owned()),
            external_order_number: Set(normalized.order_number.clone()),
            paid_amount_fen: Set(normalized.paid_amount_fen),
            ordered_at: Set(normalized.ordered_at.clone()),
            shop_name: Set(normalized.shop_name.clone()),
            raw_ocr_json: Set(normalized.raw_json.clone()),
            possible_duplicate: Set(false),
            created_at: Set(timestamp.clone()),
            updated_at: Set(timestamp.clone()),
            original_file_name: NotSet,
            managed_image_path: NotSet,
            image_sha256: NotSet,
            warning_message: NotSet,
        }
        .insert(&transaction)
        .await?;
        insert_products(&transaction, task_id, &normalized.product_names).await?;
        transaction
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "UPDATE import_tasks SET status = 'succeeded', error_code = NULL, error_message = NULL, \
                 warning_message = ?, completed_at = ? WHERE id = ? AND status = 'processing'",
                [
                    normalized.warning.clone().into(),
                    timestamp.into(),
                    task_id.into(),
                ],
            ))
            .await?;
        refresh_duplicate_group(&transaction, normalized.order_number.as_deref()).await?;
        transaction.commit().await?;
        let task = self.task(task_id).await?;
        self.refresh_batch_status(&task.batch_id).await
    }

    #[cfg(test)]
    pub async fn list_import_batches(
        &self,
        query: ImportBatchesQuery,
    ) -> AppResult<ImportBatchesPage> {
        let page = query.page.unwrap_or(1).max(1);
        let page_size = query.page_size.unwrap_or(20).clamp(1, 100);
        let total = import_batch::Entity::find().count(&self.connection).await? as i64;
        let rows = ImportBatchSummaryRow::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT b.id, b.platform, b.status, b.created_at, b.completed_at, \
             COUNT(t.id) AS total, \
             COALESCE(SUM(CASE WHEN t.status = 'queued' THEN 1 ELSE 0 END), 0) AS queued, \
             COALESCE(SUM(CASE WHEN t.status = 'processing' THEN 1 ELSE 0 END), 0) AS processing, \
             COALESCE(SUM(CASE WHEN t.status = 'succeeded' THEN 1 ELSE 0 END), 0) AS succeeded, \
             COALESCE(SUM(CASE WHEN t.status = 'failed' THEN 1 ELSE 0 END), 0) AS failed, \
             COALESCE(SUM(CASE WHEN t.status = 'canceled' THEN 1 ELSE 0 END), 0) AS canceled, \
             COALESCE(SUM(CASE WHEN t.status = 'skipped' THEN 1 ELSE 0 END), 0) AS skipped, \
             COALESCE(SUM(CASE WHEN t.status = 'interrupted' THEN 1 ELSE 0 END), 0) AS interrupted \
             FROM import_batches b LEFT JOIN import_tasks t ON t.batch_id = b.id \
             GROUP BY b.id, b.platform, b.status, b.created_at, b.completed_at \
             ORDER BY b.created_at DESC, b.id DESC LIMIT ? OFFSET ?",
            [
                i64::from(page_size).into(),
                (i64::from(page - 1) * i64::from(page_size)).into(),
            ],
        ))
        .all(&self.connection)
        .await?;

        Ok(ImportBatchesPage {
            items: rows.into_iter().map(ImportBatchSummary::from).collect(),
            total,
            page,
            page_size,
        })
    }

    #[cfg(test)]
    pub async fn list_orders(&self, query: OrdersQuery) -> AppResult<OrdersPage> {
        self.list_orders_with_matching(
            query,
            usize::from(crate::models::DEFAULT_MATCH_MAX_ORDERS),
            usize::from(crate::models::DEFAULT_MATCH_MAX_CANDIDATES),
        )
        .await
    }

    pub async fn list_orders_with_matching(
        &self,
        query: OrdersQuery,
        max_orders: usize,
        max_candidates: usize,
    ) -> AppResult<OrdersPage> {
        let page = query.page.unwrap_or(1).max(1);
        let page_size = query.page_size.unwrap_or(20).clamp(1, 100);
        let search = query.search.unwrap_or_default().trim().to_owned();
        let duplicate_only = query.duplicate_only.unwrap_or(false);
        let match_status = normalized_ledger_filter(query.match_status.as_deref())?;
        let project_status = normalized_project_filter(query.project_status.as_deref())?;
        let project_id = query
            .project_id
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        let mut filter_sql = String::new();
        let mut filter_values: Vec<sea_orm::Value> = Vec::new();
        if !search.is_empty() {
            filter_sql.push_str(
                " AND (o.external_order_number LIKE ? OR o.shop_name LIKE ? OR EXISTS \
                 (SELECT 1 FROM order_items oi WHERE oi.order_id = o.id AND oi.product_name LIKE ?))",
            );
            let pattern = format!("%{search}%");
            filter_values.extend([
                pattern.clone().into(),
                pattern.clone().into(),
                pattern.into(),
            ]);
        }
        if duplicate_only {
            filter_sql.push_str(" AND o.possible_duplicate = 1");
        }
        filter_sql.push_str(
            " AND NOT EXISTS (SELECT 1 FROM invoice_match_orders amo \
             JOIN project_invoices api ON api.invoice_id=amo.invoice_id \
             JOIN projects ap ON ap.id=api.project_id \
             WHERE amo.order_id=o.id AND ap.archived_at IS NOT NULL)",
        );
        if let Some(project_id) = &project_id {
            filter_sql.push_str(
                " AND EXISTS (SELECT 1 FROM invoice_match_orders pmo \
                 JOIN project_invoices ppi ON ppi.invoice_id=pmo.invoice_id \
                 JOIN projects pp ON pp.id=ppi.project_id \
                 WHERE pmo.order_id=o.id AND ppi.project_id=? AND pp.archived_at IS NULL)",
            );
            filter_values.push(project_id.clone().into());
        } else if project_status == Some("assigned") {
            filter_sql.push_str(
                " AND EXISTS (SELECT 1 FROM invoice_match_orders pmo \
                 JOIN project_invoices ppi ON ppi.invoice_id=pmo.invoice_id \
                 JOIN projects pp ON pp.id=ppi.project_id \
                 WHERE pmo.order_id=o.id AND pp.archived_at IS NULL)",
            );
        } else if project_status == Some("unassigned") {
            filter_sql.push_str(
                " AND NOT EXISTS (SELECT 1 FROM invoice_match_orders pmo \
                 JOIN project_invoices ppi ON ppi.invoice_id=pmo.invoice_id \
                 JOIN projects pp ON pp.id=ppi.project_id \
                 WHERE pmo.order_id=o.id AND pp.archived_at IS NULL)",
            );
        }

        let sort_column = match query.sort_by.unwrap_or(OrderSort::CreatedAt) {
            OrderSort::CreatedAt => "o.created_at",
            OrderSort::OrderedAt => "o.ordered_at",
            OrderSort::PaidAmount => "o.paid_amount_fen",
            OrderSort::ShopName => "o.shop_name",
        };
        let direction = match query.sort_direction.unwrap_or(SortDirection::Desc) {
            SortDirection::Asc => "ASC",
            SortDirection::Desc => "DESC",
        };
        let statement = Statement::from_sql_and_values(
            DbBackend::Sqlite,
            format!(
                "SELECT o.id, o.platform, o.external_order_number, o.paid_amount_fen, o.ordered_at, \
                 o.shop_name, o.possible_duplicate, o.created_at, \
                 COALESCE((SELECT group_concat(product_name, ' · ') FROM \
                    (SELECT product_name FROM order_items WHERE order_id = o.id ORDER BY position LIMIT 2)), '') AS product_summary, \
                 (SELECT COUNT(*) FROM order_items WHERE order_id = o.id) AS product_count, \
                 '' AS match_status, \
                 (SELECT ppi.project_id FROM invoice_match_orders pmo \
                   JOIN project_invoices ppi ON ppi.invoice_id=pmo.invoice_id \
                   JOIN projects pp ON pp.id=ppi.project_id \
                   WHERE pmo.order_id=o.id AND pp.archived_at IS NULL LIMIT 1) AS project_id, \
                 (SELECT pp.name FROM invoice_match_orders pmo \
                   JOIN project_invoices ppi ON ppi.invoice_id=pmo.invoice_id \
                   JOIN projects pp ON pp.id=ppi.project_id \
                   WHERE pmo.order_id=o.id AND pp.archived_at IS NULL LIMIT 1) AS project_name \
                 FROM orders o WHERE 1 = 1{filter_sql} ORDER BY {sort_column} {direction}, o.id DESC"
            ),
            filter_values,
        );
        let mut items = OrderListItem::find_by_statement(statement)
            .all(&self.connection)
            .await?;
        let projection = self
            .ledger_match_projection(max_orders, max_candidates)
            .await?;
        for item in &mut items {
            item.match_status = projection
                .order_statuses
                .get(&item.id)
                .cloned()
                .unwrap_or_else(|| "unmatched".to_owned());
        }
        if let Some(match_status) = match_status {
            items.retain(|item| item.match_status == match_status);
        }
        let total = i64::try_from(items.len()).unwrap_or(i64::MAX);
        let start = usize::try_from((page - 1) * page_size).unwrap_or(usize::MAX);
        let items = items
            .into_iter()
            .skip(start)
            .take(page_size as usize)
            .collect();
        Ok(OrdersPage {
            items,
            total,
            page,
            page_size,
        })
    }

    pub async fn order_detail(&self, id: &str) -> AppResult<OrderDetail> {
        let order_model = order::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("订单 {id}")))?;
        let product_names = order_model
            .find_related(order_item::Entity)
            .order_by_asc(order_item::Column::Position)
            .all(&self.connection)
            .await?
            .into_iter()
            .map(|item| item.product_name)
            .collect();
        let is_matched = invoice_match_order::Entity::find()
            .filter(invoice_match_order::Column::OrderId.eq(id))
            .one(&self.connection)
            .await?
            .is_some();
        Ok(OrderDetail {
            id: order_model.id,
            platform: order_model.platform,
            external_order_number: order_model.external_order_number,
            product_names,
            paid_amount_fen: order_model.paid_amount_fen,
            ordered_at: order_model.ordered_at,
            shop_name: order_model.shop_name,
            raw_ocr_json: order_model.raw_ocr_json,
            possible_duplicate: order_model.possible_duplicate,
            created_at: order_model.created_at,
            updated_at: order_model.updated_at,
            original_file_name: order_model.original_file_name.unwrap_or_default(),
            image_path: order_model.managed_image_path.unwrap_or_default(),
            warning_message: order_model.warning_message,
            is_matched,
        })
    }

    pub async fn update_order(&self, request: UpdateOrderRequest) -> AppResult<OrderDetail> {
        validate_order_update(&request)?;
        let transaction = self.connection.begin().await?;
        let model = order::Entity::find_by_id(&request.id)
            .one(&transaction)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("订单 {}", request.id)))?;
        let old_number = model.external_order_number.clone();
        let new_number = clean_optional(request.external_order_number);
        let next_ordered_at = clean_optional(request.ordered_at);
        let next_shop_name = clean_optional(request.shop_name);
        let old_product_names = order_item::Entity::find()
            .filter(order_item::Column::OrderId.eq(&request.id))
            .order_by_asc(order_item::Column::Position)
            .all(&transaction)
            .await?
            .into_iter()
            .map(|item| item.product_name)
            .collect::<Vec<_>>();
        let next_product_names = request
            .product_names
            .iter()
            .map(|name| name.trim())
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let summary_input_changed = old_product_names != next_product_names;
        let matching_fields_changed = model.paid_amount_fen != request.paid_amount_fen
            || model.ordered_at != next_ordered_at
            || model.shop_name != next_shop_name;
        let matched_link = invoice_match_order::Entity::find()
            .filter(invoice_match_order::Column::OrderId.eq(&request.id))
            .one(&transaction)
            .await?;
        if matching_fields_changed && let Some(link) = &matched_link {
            invoice_match::Entity::delete_by_id(&link.invoice_id)
                .exec(&transaction)
                .await?;
        }
        let mut active = model.into_active_model();
        active.external_order_number = Set(new_number.clone());
        active.paid_amount_fen = Set(request.paid_amount_fen);
        active.ordered_at = Set(next_ordered_at);
        active.shop_name = Set(next_shop_name);
        active.updated_at = Set(now());
        active.update(&transaction).await?;
        order_item::Entity::delete_many()
            .filter(order_item::Column::OrderId.eq(&request.id))
            .exec(&transaction)
            .await?;
        insert_products(&transaction, &request.id, &next_product_names).await?;
        if summary_input_changed
            && !matching_fields_changed
            && let Some(link) = &matched_link
        {
            crate::match_summary::reset_match_summary(&transaction, &link.invoice_id).await?;
        }
        refresh_duplicate_group(&transaction, old_number.as_deref()).await?;
        refresh_duplicate_group(&transaction, new_number.as_deref()).await?;
        transaction.commit().await?;
        self.order_detail(&request.id).await
    }

    pub async fn delete_order_record(&self, id: &str) -> AppResult<()> {
        let transaction = self.connection.begin().await?;
        let model = order::Entity::find_by_id(id)
            .one(&transaction)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("订单 {id}")))?;
        let old_number = model.external_order_number.clone();
        if invoice_match_order::Entity::find()
            .filter(invoice_match_order::Column::OrderId.eq(id))
            .one(&transaction)
            .await?
            .is_some()
        {
            return Err(AppError::MatchingLocked(format!("订单 {id}")));
        }
        let result = order::Entity::delete_by_id(id).exec(&transaction).await?;
        if result.rows_affected != 1 {
            return Err(AppError::NotFound(format!("订单 {id}")));
        }
        refresh_duplicate_group(&transaction, old_number.as_deref()).await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn image_relative_path(&self, id: &str) -> AppResult<String> {
        order::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .and_then(|order| order.managed_image_path)
            .ok_or_else(|| AppError::NotFound(format!("订单图片 {id}")))
    }

    async fn set_batch_processing(&self, batch_id: &str) -> AppResult<()> {
        import_batch::Entity::update_many()
            .col_expr(import_batch::Column::Status, Expr::value("processing"))
            .col_expr(
                import_batch::Column::CompletedAt,
                Expr::value(Option::<String>::None),
            )
            .filter(import_batch::Column::Id.eq(batch_id))
            .exec(&self.connection)
            .await?;
        Ok(())
    }

    async fn refresh_batch_status(&self, batch_id: &str) -> AppResult<()> {
        let active = import_task::Entity::find()
            .filter(import_task::Column::BatchId.eq(batch_id))
            .filter(import_task::Column::Status.is_in(["queued", "processing"]))
            .count(&self.connection)
            .await?;
        if active > 0 {
            return self.set_batch_processing(batch_id).await;
        }
        let errors = import_task::Entity::find()
            .filter(import_task::Column::BatchId.eq(batch_id))
            .filter(import_task::Column::Status.is_in(["failed", "canceled", "interrupted"]))
            .count(&self.connection)
            .await?;
        let model = import_batch::Entity::find_by_id(batch_id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("导入批次 {batch_id}")))?;
        let mut active_model = model.into_active_model();
        active_model.status = Set(if errors > 0 {
            "completed_with_errors".into()
        } else {
            "completed".into()
        });
        active_model.completed_at = Set(Some(now()));
        active_model.update(&self.connection).await?;
        Ok(())
    }

    async fn refresh_open_batches(&self) -> AppResult<()> {
        let ids = import_batch::Entity::find()
            .select_only()
            .column(import_batch::Column::Id)
            .filter(import_batch::Column::Status.is_in(["queued", "processing"]))
            .into_tuple::<String>()
            .all(&self.connection)
            .await?;
        for id in ids {
            self.refresh_batch_status(&id).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[derive(FromQueryResult)]
struct ImportBatchSummaryRow {
    id: String,
    platform: String,
    status: String,
    created_at: String,
    completed_at: Option<String>,
    total: i64,
    queued: i64,
    processing: i64,
    succeeded: i64,
    failed: i64,
    canceled: i64,
    skipped: i64,
    interrupted: i64,
}

#[cfg(test)]
impl From<ImportBatchSummaryRow> for ImportBatchSummary {
    fn from(row: ImportBatchSummaryRow) -> Self {
        Self {
            id: row.id,
            platform: row.platform,
            status: row.status,
            created_at: row.created_at,
            completed_at: row.completed_at,
            counts: BatchCounts {
                total: row.total,
                queued: row.queued,
                processing: row.processing,
                succeeded: row.succeeded,
                failed: row.failed,
                canceled: row.canceled,
                skipped: row.skipped,
                interrupted: row.interrupted,
            },
        }
    }
}

#[cfg(test)]
impl From<import_task::Model> for ImportTask {
    fn from(model: import_task::Model) -> Self {
        Self {
            id: model.id,
            batch_id: model.batch_id,
            original_file_name: model.original_file_name,
            managed_image_path: model.managed_image_path,
            image_sha256: model.image_sha256,
            status: model.status,
            error_code: model.error_code,
            error_message: model.error_message,
            warning_message: model.warning_message,
            retry_count: i64::from(model.retry_count),
            created_at: model.created_at,
            started_at: model.started_at,
            completed_at: model.completed_at,
        }
    }
}

fn settings_from_model(model: app_setting::Model) -> AppSettings {
    AppSettings {
        llm_provider: crate::models::LlmProvider::Ollama,
        ollama_base_url: model.ollama_base_url,
        ollama_model: model.ollama_model,
        openai_base_url: crate::models::DEFAULT_OPENAI_BASE_URL.into(),
        openai_model: crate::models::DEFAULT_OPENAI_MODEL.into(),
        openai_api_key_masked: None,
        openai_api_key_configured: false,
        order_recognition_max_concurrency: u8::try_from(model.max_concurrency)
            .unwrap_or(DEFAULT_MAX_CONCURRENCY),
        summary_generation_mode: crate::models::SummaryGenerationMode::Automatic,
        summary_generation_max_concurrency: crate::models::DEFAULT_SUMMARY_GENERATION_CONCURRENCY,
        tencent_secret_id_masked: None,
        tencent_credentials_configured: false,
        invoice_ocr_concurrency: crate::models::DEFAULT_INVOICE_OCR_CONCURRENCY,
        match_max_orders: crate::models::DEFAULT_MATCH_MAX_ORDERS,
        match_max_candidates: crate::models::DEFAULT_MATCH_MAX_CANDIDATES,
        settings_path: String::new(),
        permission_warning: None,
    }
}

pub(crate) async fn insert_products<C: ConnectionTrait>(
    connection: &C,
    order_id: &str,
    product_names: &[String],
) -> AppResult<()> {
    let models = product_names
        .iter()
        .enumerate()
        .filter_map(|(position, name)| {
            let name = name.trim();
            (!name.is_empty()).then(|| order_item::ActiveModel {
                id: NotSet,
                order_id: Set(order_id.to_owned()),
                position: Set(position as i32),
                product_name: Set(name.to_owned()),
            })
        })
        .collect::<Vec<_>>();
    if !models.is_empty() {
        order_item::Entity::insert_many(models)
            .exec(connection)
            .await?;
    }
    Ok(())
}

pub(crate) async fn refresh_duplicate_group(
    transaction: &DatabaseTransaction,
    order_number: Option<&str>,
) -> AppResult<()> {
    let Some(order_number) = order_number.filter(|value| !value.trim().is_empty()) else {
        return Ok(());
    };
    transaction
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE orders SET possible_duplicate = CASE \
             WHEN (SELECT COUNT(*) FROM orders grouped WHERE grouped.external_order_number = ?) > 1 \
             THEN 1 ELSE 0 END WHERE external_order_number = ?",
            [order_number.into(), order_number.into()],
        ))
        .await?;
    Ok(())
}

fn validate_order_update(request: &UpdateOrderRequest) -> AppResult<()> {
    if request.product_names.len() > 100 {
        return Err(AppError::InvalidInput("单个订单最多包含 100 个商品".into()));
    }
    if request.paid_amount_fen.is_some_and(|amount| amount < 0) {
        return Err(AppError::InvalidInput("实付金额不能为负数".into()));
    }
    Ok(())
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value.and_then(|text| {
        let text = text.trim().to_owned();
        (!text.is_empty()).then_some(text)
    })
}

pub(crate) fn now() -> String {
    Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    async fn seeded_database() -> Database {
        Database::in_memory().await.unwrap()
    }

    async fn insert_recognized(db: &Database, id: &str, number: &str) {
        db.create_batch("batch", Platform::Taobao).await.ok();
        db.create_task(
            id,
            "batch",
            "order.png",
            Some(&format!("images/{id}.png")),
            id,
            "queued",
            None,
        )
        .await
        .unwrap();
        db.mark_processing(id).await.unwrap();
        db.persist_recognized_order(
            id,
            Platform::Taobao,
            &NormalizedOrder {
                order_number: Some(number.into()),
                product_names: vec!["商品".into()],
                paid_amount_fen: Some(100),
                ordered_at: None,
                shop_name: None,
                warning: None,
                raw_json: json!({"order_number": number}).to_string(),
            },
        )
        .await
        .unwrap();
    }

    async fn insert_batch_at(db: &Database, id: &str, created_at: &str) {
        import_batch::ActiveModel {
            id: Set(id.into()),
            platform: Set("taobao".into()),
            status: Set("completed_with_errors".into()),
            created_at: Set(created_at.into()),
            completed_at: Set(Some(created_at.into())),
        }
        .insert(&db.connection)
        .await
        .unwrap();
    }

    async fn insert_task_with_status(db: &Database, id: &str, batch_id: &str, status: &str) {
        db.create_task(id, batch_id, "order.png", None, id, status, None)
            .await
            .unwrap();
    }

    struct QueryOrderSeed<'a> {
        id: &'a str,
        number: &'a str,
        product: &'a str,
        shop: Option<&'a str>,
        amount: Option<i64>,
        ordered_at: Option<&'a str>,
        created_at: &'a str,
        duplicate: bool,
    }

    async fn insert_query_order(db: &Database, seed: QueryOrderSeed<'_>) {
        db.create_batch("query-batch", Platform::Taobao).await.ok();
        db.create_task(
            seed.id,
            "query-batch",
            "order.png",
            None,
            seed.id,
            "succeeded",
            None,
        )
        .await
        .unwrap();
        order::ActiveModel {
            id: Set(seed.id.into()),
            platform: Set("taobao".into()),
            external_order_number: Set(Some(seed.number.into())),
            paid_amount_fen: Set(seed.amount),
            ordered_at: Set(seed.ordered_at.map(str::to_owned)),
            shop_name: Set(seed.shop.map(str::to_owned)),
            raw_ocr_json: Set("{}".into()),
            possible_duplicate: Set(seed.duplicate),
            created_at: Set(seed.created_at.into()),
            updated_at: Set(seed.created_at.into()),
            original_file_name: Set(Some("order.png".into())),
            managed_image_path: Set(None),
            image_sha256: Set(None),
            warning_message: Set(None),
        }
        .insert(&db.connection)
        .await
        .unwrap();
        insert_products(&db.connection, seed.id, &[seed.product.into()])
            .await
            .unwrap();
    }

    fn order_query(sort_by: OrderSort, sort_direction: SortDirection) -> OrdersQuery {
        OrdersQuery {
            page: Some(1),
            page_size: Some(100),
            search: None,
            duplicate_only: None,
            match_status: None,
            project_status: None,
            project_id: None,
            sort_by: Some(sort_by),
            sort_direction: Some(sort_direction),
        }
    }

    async fn seeded_orders_for_query() -> Database {
        let db = seeded_database().await;
        for seed in [
            QueryOrderSeed {
                id: "order-a",
                number: "1001",
                product: "键盘",
                shop: Some("北店"),
                amount: Some(3000),
                ordered_at: Some("2026-08-10 09:00"),
                created_at: "2026-08-12T08:00:00Z",
                duplicate: false,
            },
            QueryOrderSeed {
                id: "order-b",
                number: "1002",
                product: "显示器",
                shop: Some("南店"),
                amount: Some(1000),
                ordered_at: Some("2026-08-11 09:00"),
                created_at: "2026-08-12T09:00:00Z",
                duplicate: true,
            },
            QueryOrderSeed {
                id: "order-c",
                number: "1003",
                product: "鼠标",
                shop: Some("北店"),
                amount: Some(2000),
                ordered_at: Some("2026-08-12 09:00"),
                created_at: "2026-08-12T09:00:00Z",
                duplicate: false,
            },
        ] {
            insert_query_order(&db, seed).await;
        }
        db
    }

    #[tokio::test]
    async fn import_batch_history_is_stably_paged_with_aggregate_counts() {
        let db = seeded_database().await;
        insert_batch_at(&db, "batch-a", "2026-08-10T09:00:00Z").await;
        insert_batch_at(&db, "batch-b", "2026-08-11T09:00:00Z").await;
        insert_batch_at(&db, "batch-c", "2026-08-11T09:00:00Z").await;

        for (index, status) in [
            "queued",
            "processing",
            "succeeded",
            "failed",
            "canceled",
            "skipped",
            "interrupted",
        ]
        .into_iter()
        .enumerate()
        {
            insert_task_with_status(&db, &format!("task-{index}"), "batch-c", status).await;
        }

        let first = db
            .list_import_batches(ImportBatchesQuery {
                page: Some(1),
                page_size: Some(2),
            })
            .await
            .unwrap();
        assert_eq!(first.total, 3);
        assert_eq!(first.page, 1);
        assert_eq!(first.page_size, 2);
        assert_eq!(
            first
                .items
                .iter()
                .map(|batch| batch.id.as_str())
                .collect::<Vec<_>>(),
            vec!["batch-c", "batch-b"]
        );
        assert_eq!(first.items[0].counts.total, 7);
        assert_eq!(first.items[0].counts.queued, 1);
        assert_eq!(first.items[0].counts.processing, 1);
        assert_eq!(first.items[0].counts.succeeded, 1);
        assert_eq!(first.items[0].counts.failed, 1);
        assert_eq!(first.items[0].counts.canceled, 1);
        assert_eq!(first.items[0].counts.skipped, 1);
        assert_eq!(first.items[0].counts.interrupted, 1);
        assert_eq!(first.items[1].counts.total, 0);

        let second = db
            .list_import_batches(ImportBatchesQuery {
                page: Some(2),
                page_size: Some(2),
            })
            .await
            .unwrap();
        assert_eq!(second.items.len(), 1);
        assert_eq!(second.items[0].id, "batch-a");
    }

    #[tokio::test]
    async fn import_batch_history_normalizes_page_bounds() {
        let db = seeded_database().await;
        let page = db
            .list_import_batches(ImportBatchesQuery {
                page: Some(0),
                page_size: Some(500),
            })
            .await
            .unwrap();
        assert_eq!(page.total, 0);
        assert!(page.items.is_empty());
        assert_eq!(page.page, 1);
        assert_eq!(page.page_size, 100);
    }

    #[tokio::test]
    async fn orders_support_every_sort_field_in_both_directions() {
        let db = seeded_orders_for_query().await;
        let cases = [
            (
                OrderSort::CreatedAt,
                vec!["order-a", "order-c", "order-b"],
                vec!["order-c", "order-b", "order-a"],
            ),
            (
                OrderSort::OrderedAt,
                vec!["order-a", "order-b", "order-c"],
                vec!["order-c", "order-b", "order-a"],
            ),
            (
                OrderSort::PaidAmount,
                vec!["order-b", "order-c", "order-a"],
                vec!["order-a", "order-c", "order-b"],
            ),
            (
                OrderSort::ShopName,
                vec!["order-c", "order-a", "order-b"],
                vec!["order-b", "order-c", "order-a"],
            ),
        ];

        for (sort, ascending_ids, descending_ids) in cases {
            let ascending = db
                .list_orders(order_query(sort, SortDirection::Asc))
                .await
                .unwrap();
            assert_eq!(
                ascending
                    .items
                    .iter()
                    .map(|order| order.id.as_str())
                    .collect::<Vec<_>>(),
                ascending_ids,
            );

            let descending = db
                .list_orders(order_query(sort, SortDirection::Desc))
                .await
                .unwrap();
            assert_eq!(
                descending
                    .items
                    .iter()
                    .map(|order| order.id.as_str())
                    .collect::<Vec<_>>(),
                descending_ids,
            );
        }
    }

    #[tokio::test]
    async fn order_filters_combine_with_stable_pagination() {
        let db = seeded_orders_for_query().await;
        let duplicate_page = db
            .list_orders(OrdersQuery {
                page: Some(1),
                page_size: Some(1),
                search: Some("显示器".into()),
                duplicate_only: Some(true),
                match_status: None,
                project_status: None,
                project_id: None,
                sort_by: Some(OrderSort::CreatedAt),
                sort_direction: Some(SortDirection::Desc),
            })
            .await
            .unwrap();
        assert_eq!(duplicate_page.total, 1);
        assert_eq!(duplicate_page.items[0].id, "order-b");

        let tied_page = db
            .list_orders(OrdersQuery {
                page: Some(2),
                page_size: Some(1),
                search: None,
                duplicate_only: None,
                match_status: None,
                project_status: None,
                project_id: None,
                sort_by: Some(OrderSort::CreatedAt),
                sort_direction: Some(SortDirection::Desc),
            })
            .await
            .unwrap();
        assert_eq!(tied_page.total, 3);
        assert_eq!(tied_page.items[0].id, "order-b");
    }

    #[tokio::test]
    async fn duplicate_order_numbers_are_retained_and_marked() {
        let db = seeded_database().await;
        insert_recognized(&db, "019c0000-0000-7000-8000-000000000001", "123").await;
        insert_recognized(&db, "019c0000-0000-7000-8000-000000000002", "123").await;
        let first = db
            .order_detail("019c0000-0000-7000-8000-000000000001")
            .await
            .unwrap();
        let second = db
            .order_detail("019c0000-0000-7000-8000-000000000002")
            .await
            .unwrap();
        assert!(first.possible_duplicate && second.possible_duplicate);
    }

    #[tokio::test]
    async fn editing_number_recalculates_old_duplicate_group() {
        let db = seeded_database().await;
        insert_recognized(&db, "019c0000-0000-7000-8000-000000000001", "123").await;
        insert_recognized(&db, "019c0000-0000-7000-8000-000000000002", "123").await;
        db.update_order(UpdateOrderRequest {
            id: "019c0000-0000-7000-8000-000000000002".into(),
            external_order_number: Some("456".into()),
            product_names: vec!["商品".into()],
            paid_amount_fen: Some(100),
            ordered_at: None,
            shop_name: None,
        })
        .await
        .unwrap();
        assert!(
            !db.order_detail("019c0000-0000-7000-8000-000000000001")
                .await
                .unwrap()
                .possible_duplicate
        );
    }
}
