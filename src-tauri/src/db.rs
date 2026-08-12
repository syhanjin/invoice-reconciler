use std::path::Path;

use chrono::Utc;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ActiveValue::NotSet, ColumnTrait, ConnectOptions, ConnectionTrait,
    DatabaseConnection, DatabaseTransaction, DbBackend, EntityTrait, FromQueryResult,
    IntoActiveModel, ModelTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set,
    Statement, TransactionTrait,
};
use sea_orm_migration::MigratorTrait;

use crate::entities::{app_setting, import_batch, import_task, order, order_item};
use crate::error::{AppError, AppResult};
use crate::migration::Migrator;
use crate::models::{
    AppSettings, BatchCounts, BatchDetail, DEFAULT_MAX_CONCURRENCY, ImportBatchSummary,
    ImportBatchesPage, ImportBatchesQuery, ImportTask, NormalizedOrder, OrderDetail, OrderListItem,
    OrderSort, OrdersPage, OrdersQuery, Platform, SortDirection, UpdateOrderRequest,
    UpdateSettingsRequest,
};

#[derive(Clone)]
pub struct Database {
    connection: DatabaseConnection,
}

impl Database {
    pub async fn connect(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut options = ConnectOptions::new(format!("sqlite://{}?mode=rwc", path.display()));
        options.max_connections(5).sqlx_logging(false);
        let connection = sea_orm::Database::connect(options).await?;
        connection
            .execute(Statement::from_string(
                DbBackend::Sqlite,
                "PRAGMA foreign_keys = ON",
            ))
            .await?;
        Migrator::up(&connection, None).await?;
        connection
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "UPDATE import_tasks SET status = 'interrupted', error_code = 'app_interrupted', \
                 error_message = '应用在识别完成前退出，可手动重试', completed_at = ? \
                 WHERE status IN ('queued', 'processing')",
                [now().into()],
            ))
            .await?;
        let database = Self { connection };
        database.refresh_open_batches().await?;
        Ok(database)
    }

    #[cfg(test)]
    pub async fn in_memory() -> AppResult<Self> {
        let mut options = ConnectOptions::new("sqlite::memory:");
        options.max_connections(1).sqlx_logging(false);
        let connection = sea_orm::Database::connect(options).await?;
        connection
            .execute(Statement::from_string(
                DbBackend::Sqlite,
                "PRAGMA foreign_keys = ON",
            ))
            .await?;
        Migrator::up(&connection, None).await?;
        Ok(Self { connection })
    }

    pub async fn settings(&self) -> AppResult<AppSettings> {
        let model = app_setting::Entity::find_by_id(1)
            .one(&self.connection)
            .await?
            .ok_or_else(|| AppError::Database("缺少应用设置记录".into()))?;
        Ok(settings_from_model(model))
    }

    pub async fn update_settings(&self, request: UpdateSettingsRequest) -> AppResult<AppSettings> {
        let request = request.validate()?;
        let model = app_setting::Entity::find_by_id(1)
            .one(&self.connection)
            .await?
            .ok_or_else(|| AppError::Database("缺少应用设置记录".into()))?;
        let mut active = model.into_active_model();
        active.ollama_base_url = Set(request.ollama_base_url);
        active.ollama_model = Set(request.ollama_model);
        active.max_concurrency = Set(i32::from(request.max_concurrency));
        active.updated_at = Set(now());
        Ok(settings_from_model(active.update(&self.connection).await?))
    }

    pub async fn hash_exists(&self, hash: &str) -> AppResult<bool> {
        Ok(import_task::Entity::find()
            .filter(import_task::Column::ImageSha256.eq(hash))
            .filter(import_task::Column::ManagedImagePath.is_not_null())
            .one(&self.connection)
            .await?
            .is_some())
    }

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

    pub async fn delete_batch(&self, id: &str) -> AppResult<()> {
        import_batch::Entity::delete_by_id(id)
            .exec(&self.connection)
            .await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
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

    pub async fn task(&self, id: &str) -> AppResult<ImportTask> {
        import_task::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .map(ImportTask::from)
            .ok_or_else(|| AppError::NotFound(format!("导入任务 {id}")))
    }

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

    pub async fn mark_failed(&self, id: &str, code: &str, message: &str) -> AppResult<()> {
        let task = self.task(id).await?;
        let result = self
            .connection
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "UPDATE import_tasks SET status = 'failed', error_code = ?, error_message = ?, \
                 completed_at = ? WHERE id = ?",
                [code.into(), message.into(), now().into(), id.into()],
            ))
            .await?;
        if result.rows_affected() != 1 {
            return Err(AppError::NotFound(format!("导入任务 {id}")));
        }
        self.refresh_batch_status(&task.batch_id).await
    }

    pub async fn mark_canceled(&self, id: &str) -> AppResult<bool> {
        let result = self
            .connection
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "UPDATE import_tasks SET status = 'canceled', error_code = NULL, error_message = NULL, \
                 completed_at = ? WHERE id = ? AND status = 'queued'",
                [now().into(), id.into()],
            ))
            .await?;
        if result.rows_affected() == 1 {
            let task = self.task(id).await?;
            self.refresh_batch_status(&task.batch_id).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub async fn cancel_batch_queued(&self, batch_id: &str) -> AppResult<u64> {
        let result = self
            .connection
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "UPDATE import_tasks SET status = 'canceled', completed_at = ? \
                 WHERE batch_id = ? AND status = 'queued'",
                [now().into(), batch_id.into()],
            ))
            .await?;
        self.refresh_batch_status(batch_id).await?;
        Ok(result.rows_affected())
    }

    pub async fn prepare_retry(&self, id: &str) -> AppResult<ImportTask> {
        let result = self
            .connection
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "UPDATE import_tasks SET status = 'queued', error_code = NULL, error_message = NULL, \
                 warning_message = NULL, retry_count = retry_count + 1, started_at = NULL, completed_at = NULL \
                 WHERE id = ? AND status IN ('failed', 'interrupted')",
                [id.into()],
            ))
            .await?;
        if result.rows_affected() != 1 {
            return Err(AppError::InvalidInput(
                "只有失败或已中断的任务可以重试".into(),
            ));
        }
        let task = self.task(id).await?;
        self.set_batch_processing(&task.batch_id).await?;
        Ok(task)
    }

    pub async fn prepare_retries(&self, ids: &[String]) -> AppResult<()> {
        let transaction = self.connection.begin().await?;
        for id in ids {
            let result = transaction
                .execute(Statement::from_sql_and_values(
                    DbBackend::Sqlite,
                    "UPDATE import_tasks SET status = 'queued', error_code = NULL, error_message = NULL, \
                     warning_message = NULL, retry_count = retry_count + 1, started_at = NULL, completed_at = NULL \
                     WHERE id = ? AND status IN ('failed', 'interrupted')",
                    [id.as_str().into()],
                ))
                .await?;
            if result.rows_affected() != 1 {
                transaction.rollback().await?;
                return Err(AppError::InvalidInput(format!(
                    "任务 {id} 已不处于可重试状态"
                )));
            }
        }
        transaction.commit().await?;
        Ok(())
    }

    pub async fn retryable_task_ids(&self, batch_id: &str) -> AppResult<Vec<String>> {
        Ok(import_task::Entity::find()
            .select_only()
            .column(import_task::Column::Id)
            .filter(import_task::Column::BatchId.eq(batch_id))
            .filter(import_task::Column::Status.is_in(["failed", "interrupted"]))
            .order_by_asc(import_task::Column::CreatedAt)
            .into_tuple::<String>()
            .all(&self.connection)
            .await?)
    }

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

    pub async fn batch(&self, id: &str) -> AppResult<BatchDetail> {
        let batch = import_batch::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("导入批次 {id}")))?;
        let tasks = batch
            .find_related(import_task::Entity)
            .order_by_asc(import_task::Column::CreatedAt)
            .order_by_asc(import_task::Column::Id)
            .all(&self.connection)
            .await?
            .into_iter()
            .map(ImportTask::from)
            .collect::<Vec<_>>();
        let counts = counts_from_tasks(&tasks);
        Ok(BatchDetail {
            id: batch.id,
            platform: batch.platform,
            status: batch.status,
            created_at: batch.created_at,
            completed_at: batch.completed_at,
            counts,
            tasks,
        })
    }

    pub async fn latest_batch(&self) -> AppResult<Option<BatchDetail>> {
        let id = import_batch::Entity::find()
            .select_only()
            .column(import_batch::Column::Id)
            .order_by_desc(import_batch::Column::CreatedAt)
            .into_tuple::<String>()
            .one(&self.connection)
            .await?;
        match id {
            Some(id) => self.batch(&id).await.map(Some),
            None => Ok(None),
        }
    }

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

    pub async fn list_orders(&self, query: OrdersQuery) -> AppResult<OrdersPage> {
        let page = query.page.unwrap_or(1).max(1);
        let page_size = query.page_size.unwrap_or(20).clamp(1, 100);
        let search = query.search.unwrap_or_default().trim().to_owned();
        let duplicate_only = query.duplicate_only.unwrap_or(false);
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
        let count_statement = Statement::from_sql_and_values(
            DbBackend::Sqlite,
            format!("SELECT COUNT(*) AS count FROM orders o WHERE 1 = 1{filter_sql}"),
            filter_values.clone(),
        );
        #[derive(FromQueryResult)]
        struct CountRow {
            count: i64,
        }
        let total = CountRow::find_by_statement(count_statement)
            .one(&self.connection)
            .await?
            .map(|row| row.count)
            .unwrap_or(0);

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
        let mut values = filter_values;
        values.extend([
            i64::from(page_size).into(),
            i64::from((page - 1) * page_size).into(),
        ]);
        let statement = Statement::from_sql_and_values(
            DbBackend::Sqlite,
            format!(
                "SELECT o.id, o.platform, o.external_order_number, o.paid_amount_fen, o.ordered_at, \
                 o.shop_name, o.possible_duplicate, o.created_at, \
                 COALESCE((SELECT group_concat(product_name, ' · ') FROM \
                    (SELECT product_name FROM order_items WHERE order_id = o.id ORDER BY position LIMIT 2)), '') AS product_summary, \
                 (SELECT COUNT(*) FROM order_items WHERE order_id = o.id) AS product_count \
                 FROM orders o WHERE 1 = 1{filter_sql} ORDER BY {sort_column} {direction}, o.id DESC LIMIT ? OFFSET ?"
            ),
            values,
        );
        let items = OrderListItem::find_by_statement(statement)
            .all(&self.connection)
            .await?;
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
        let task = order_model
            .find_related(import_task::Entity)
            .one(&self.connection)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("订单图片 {id}")))?;
        let product_names = order_model
            .find_related(order_item::Entity)
            .order_by_asc(order_item::Column::Position)
            .all(&self.connection)
            .await?
            .into_iter()
            .map(|item| item.product_name)
            .collect();
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
            original_file_name: task.original_file_name,
            image_path: task.managed_image_path.unwrap_or_default(),
            warning_message: task.warning_message,
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
        let mut active = model.into_active_model();
        active.external_order_number = Set(new_number.clone());
        active.paid_amount_fen = Set(request.paid_amount_fen);
        active.ordered_at = Set(clean_optional(request.ordered_at));
        active.shop_name = Set(clean_optional(request.shop_name));
        active.updated_at = Set(now());
        active.update(&transaction).await?;
        order_item::Entity::delete_many()
            .filter(order_item::Column::OrderId.eq(&request.id))
            .exec(&transaction)
            .await?;
        insert_products(&transaction, &request.id, &request.product_names).await?;
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
        let result = import_task::Entity::delete_by_id(id)
            .exec(&transaction)
            .await?;
        if result.rows_affected != 1 {
            return Err(AppError::NotFound(format!("订单 {id}")));
        }
        refresh_duplicate_group(&transaction, old_number.as_deref()).await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn image_relative_path(&self, id: &str) -> AppResult<String> {
        import_task::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .and_then(|task| task.managed_image_path)
            .ok_or_else(|| AppError::NotFound(format!("订单图片 {id}")))
    }

    pub async fn mark_batch_processing(&self, batch_id: &str) -> AppResult<()> {
        self.set_batch_processing(batch_id).await
    }

    pub async fn finalize_batch(&self, batch_id: &str) -> AppResult<()> {
        self.refresh_batch_status(batch_id).await
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
        ollama_base_url: model.ollama_base_url,
        ollama_model: model.ollama_model,
        max_concurrency: u8::try_from(model.max_concurrency).unwrap_or(DEFAULT_MAX_CONCURRENCY),
    }
}

async fn insert_products<C: ConnectionTrait>(
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

async fn refresh_duplicate_group(
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

fn counts_from_tasks(tasks: &[ImportTask]) -> BatchCounts {
    let mut counts = BatchCounts {
        total: tasks.len() as i64,
        ..Default::default()
    };
    for task in tasks {
        match task.status.as_str() {
            "queued" => counts.queued += 1,
            "processing" => counts.processing += 1,
            "succeeded" => counts.succeeded += 1,
            "failed" => counts.failed += 1,
            "canceled" => counts.canceled += 1,
            "skipped" => counts.skipped += 1,
            "interrupted" => counts.interrupted += 1,
            _ => {}
        }
    }
    counts
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

fn now() -> String {
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
