use crate::db::Database;
use crate::entities::{order, order_import_log, order_import_pool};
use crate::error::{AppError, AppResult};
use crate::models::{
    NormalizedOrder, OrderImportLog, OrderImportLogsPage, OrderImportLogsQuery, OrderImportPool,
    OrderImportTask, Platform,
};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, FromQueryResult,
    PaginatorTrait, QueryFilter, QueryOrder, Set, Statement, TransactionTrait,
};

const LEGACY_POOL_BATCH: &str = "__unified_order_pool__";

impl Database {
    #[cfg(test)]
    pub async fn create_pool_task(
        &self,
        id: &str,
        original_file_name: &str,
        managed_image_path: &str,
        image_sha256: &str,
    ) -> AppResult<()> {
        order_import_pool::ActiveModel {
            id: Set(id.to_owned()),
            original_file_name: Set(original_file_name.to_owned()),
            managed_image_path: Set(managed_image_path.to_owned()),
            image_sha256: Set(image_sha256.to_owned()),
            status: Set("pending".into()),
            error_code: Set(None),
            error_message: Set(None),
            retry_count: Set(0),
            created_at: Set(now()),
            started_at: Set(None),
        }
        .insert(&self.connection)
        .await?;
        Ok(())
    }

    pub async fn create_pool_tasks(
        &self,
        tasks: &[(String, String, String, String)],
    ) -> AppResult<()> {
        let transaction = self.connection.begin().await?;
        for (id, original_file_name, managed_image_path, image_sha256) in tasks {
            order_import_pool::ActiveModel {
                id: Set(id.clone()),
                original_file_name: Set(original_file_name.clone()),
                managed_image_path: Set(managed_image_path.clone()),
                image_sha256: Set(image_sha256.clone()),
                status: Set("pending".into()),
                error_code: Set(None),
                error_message: Set(None),
                retry_count: Set(0),
                created_at: Set(now()),
                started_at: Set(None),
            }
            .insert(&transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    pub async fn pool_task(&self, id: &str) -> AppResult<order_import_pool::Model> {
        order_import_pool::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("订单识别任务 {id}")))
    }

    pub async fn order_import_pool(&self) -> AppResult<OrderImportPool> {
        #[derive(FromQueryResult)]
        struct TaskRow {
            id: String,
            original_file_name: String,
            status: String,
            error_code: Option<String>,
            error_message: Option<String>,
            retry_count: i64,
            created_at: String,
            started_at: Option<String>,
        }
        let tasks = TaskRow::find_by_statement(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT id, original_file_name, status, error_code, error_message, retry_count, created_at, started_at FROM order_import_pool ORDER BY created_at ASC, id ASC",
        ))
        .all(&self.connection)
        .await?
        .into_iter()
        .map(|row| OrderImportTask {
            id: row.id,
            original_file_name: row.original_file_name,
            status: row.status,
            error_code: row.error_code,
            error_message: row.error_message,
            retry_count: row.retry_count,
            created_at: row.created_at,
            started_at: row.started_at,
        })
        .collect::<Vec<_>>();
        Ok(OrderImportPool {
            pending: tasks.iter().filter(|task| task.status == "pending").count() as i64,
            processing: tasks
                .iter()
                .filter(|task| task.status == "processing")
                .count() as i64,
            failed: tasks.iter().filter(|task| task.status == "failed").count() as i64,
            tasks,
        })
    }

    pub async fn claim_next_realtime_task(&self) -> AppResult<Option<order_import_pool::Model>> {
        let transaction = self.connection.begin().await?;
        let Some(task) = order_import_pool::Entity::find()
            .filter(order_import_pool::Column::Status.eq("pending"))
            .order_by_asc(order_import_pool::Column::CreatedAt)
            .order_by_asc(order_import_pool::Column::Id)
            .one(&transaction)
            .await?
        else {
            transaction.commit().await?;
            return Ok(None);
        };
        let result = order_import_pool::Entity::update_many()
            .col_expr(order_import_pool::Column::Status, Expr::value("processing"))
            .col_expr(
                order_import_pool::Column::StartedAt,
                Expr::value(Some(now())),
            )
            .filter(order_import_pool::Column::Id.eq(&task.id))
            .filter(order_import_pool::Column::Status.eq("pending"))
            .exec(&transaction)
            .await?;
        transaction.commit().await?;
        if result.rows_affected == 1 {
            self.pool_task(&task.id).await.map(Some)
        } else {
            Ok(None)
        }
    }

    pub async fn fail_pool_task(&self, id: &str, code: &str, message: &str) -> AppResult<()> {
        let result = order_import_pool::Entity::update_many()
            .col_expr(order_import_pool::Column::Status, Expr::value("failed"))
            .col_expr(
                order_import_pool::Column::ErrorCode,
                Expr::value(Some(code.to_owned())),
            )
            .col_expr(
                order_import_pool::Column::ErrorMessage,
                Expr::value(Some(message.to_owned())),
            )
            .filter(order_import_pool::Column::Id.eq(id))
            .exec(&self.connection)
            .await?;
        if result.rows_affected == 0 {
            if order_import_log::Entity::find_by_id(id)
                .one(&self.connection)
                .await?
                .is_some()
            {
                return Ok(());
            }
            return Err(AppError::NotFound(format!("订单识别任务 {id}")));
        }
        Ok(())
    }

    pub async fn retry_pool_task(&self, id: &str) -> AppResult<bool> {
        let result = order_import_pool::Entity::update_many()
            .col_expr(order_import_pool::Column::Status, Expr::value("pending"))
            .col_expr(
                order_import_pool::Column::ErrorCode,
                Expr::value(Option::<String>::None),
            )
            .col_expr(
                order_import_pool::Column::ErrorMessage,
                Expr::value(Option::<String>::None),
            )
            .col_expr(
                order_import_pool::Column::RetryCount,
                Expr::col(order_import_pool::Column::RetryCount).add(1),
            )
            .col_expr(
                order_import_pool::Column::StartedAt,
                Expr::value(Option::<String>::None),
            )
            .filter(order_import_pool::Column::Id.eq(id))
            .filter(order_import_pool::Column::Status.eq("failed"))
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected == 1)
    }

    pub async fn retry_all_pool_tasks(&self) -> AppResult<u64> {
        let result = order_import_pool::Entity::update_many()
            .col_expr(order_import_pool::Column::Status, Expr::value("pending"))
            .col_expr(
                order_import_pool::Column::ErrorCode,
                Expr::value(Option::<String>::None),
            )
            .col_expr(
                order_import_pool::Column::ErrorMessage,
                Expr::value(Option::<String>::None),
            )
            .col_expr(
                order_import_pool::Column::RetryCount,
                Expr::col(order_import_pool::Column::RetryCount).add(1),
            )
            .col_expr(
                order_import_pool::Column::StartedAt,
                Expr::value(Option::<String>::None),
            )
            .filter(order_import_pool::Column::Status.eq("failed"))
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected)
    }

    pub async fn delete_pool_task_record(&self, id: &str) -> AppResult<()> {
        let result = order_import_pool::Entity::delete_many()
            .filter(order_import_pool::Column::Id.eq(id))
            .filter(order_import_pool::Column::Status.is_in(["pending", "failed"]))
            .exec(&self.connection)
            .await?;
        if result.rows_affected != 1 {
            return Err(AppError::InvalidInput(
                "只有未锁定的待处理或失败任务可以删除".into(),
            ));
        }
        Ok(())
    }

    pub async fn persist_pool_order(
        &self,
        task_id: &str,
        normalized: &NormalizedOrder,
        provider: &str,
        model: &str,
    ) -> AppResult<bool> {
        let transaction = self.connection.begin().await?;
        if order_import_log::Entity::find_by_id(task_id)
            .one(&transaction)
            .await?
            .is_some()
        {
            transaction.commit().await?;
            return Ok(false);
        }
        let task = order_import_pool::Entity::find_by_id(task_id)
            .one(&transaction)
            .await?;
        let Some(task) = task else {
            return Err(AppError::NotFound(format!("订单识别任务 {task_id}")));
        };
        if task.status != "processing" {
            return Err(AppError::InvalidInput("任务不处于可完成状态".into()));
        }
        let timestamp = now();
        let _ = transaction.execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT OR IGNORE INTO import_batches (id, platform, status, created_at, completed_at) VALUES (?, 'taobao', 'completed', ?, ?)",
            [LEGACY_POOL_BATCH.into(), timestamp.clone().into(), timestamp.clone().into()],
        )).await?;
        let _ = transaction.execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT OR IGNORE INTO import_tasks (id, batch_id, original_file_name, managed_image_path, image_sha256, status, warning_message, retry_count, created_at, completed_at) VALUES (?, ?, ?, NULL, ?, 'succeeded', ?, ?, ?, ?)",
            [task.id.clone().into(), LEGACY_POOL_BATCH.into(), task.original_file_name.clone().into(), task.image_sha256.clone().into(), normalized.warning.clone().into(), task.retry_count.into(), task.created_at.clone().into(), timestamp.clone().into()],
        )).await?;
        order::ActiveModel {
            id: Set(task.id.clone()),
            platform: Set(Platform::Taobao.as_str().into()),
            external_order_number: Set(normalized.order_number.clone()),
            paid_amount_fen: Set(normalized.paid_amount_fen),
            ordered_at: Set(normalized.ordered_at.clone()),
            shop_name: Set(normalized.shop_name.clone()),
            raw_ocr_json: Set(normalized.raw_json.clone()),
            possible_duplicate: Set(false),
            created_at: Set(timestamp.clone()),
            updated_at: Set(timestamp.clone()),
            original_file_name: Set(Some(task.original_file_name.clone())),
            managed_image_path: Set(Some(task.managed_image_path.clone())),
            image_sha256: Set(Some(task.image_sha256.clone())),
            warning_message: Set(normalized.warning.clone()),
        }
        .insert(&transaction)
        .await?;
        super::db::insert_products(&transaction, task_id, &normalized.product_names).await?;
        order_import_log::ActiveModel {
            task_id: Set(task.id.clone()),
            order_id: Set(task.id.clone()),
            original_file_name: Set(task.original_file_name),
            provider: Set(provider.to_owned()),
            model: Set(model.to_owned()),
            warning_message: Set(normalized.warning.clone()),
            completed_at: Set(timestamp),
        }
        .insert(&transaction)
        .await?;
        order_import_pool::Entity::delete_by_id(task_id)
            .exec(&transaction)
            .await?;
        super::db::refresh_duplicate_group(&transaction, normalized.order_number.as_deref())
            .await?;
        transaction.commit().await?;
        Ok(true)
    }

    pub async fn order_import_logs(
        &self,
        query: OrderImportLogsQuery,
    ) -> AppResult<OrderImportLogsPage> {
        let page = query.page.unwrap_or(1).max(1);
        let page_size = query.page_size.unwrap_or(20).clamp(1, 100);
        let total = order_import_log::Entity::find()
            .count(&self.connection)
            .await? as i64;
        let items = OrderImportLog::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT task_id, order_id, original_file_name, provider, model, warning_message, completed_at FROM order_import_logs ORDER BY completed_at DESC, task_id DESC LIMIT ? OFFSET ?",
            [i64::from(page_size).into(), i64::from((page - 1) * page_size).into()],
        ))
        .all(&self.connection)
        .await?;
        Ok(OrderImportLogsPage {
            items,
            total,
            page,
            page_size,
        })
    }
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::OrderImportLogsQuery;

    fn normalized(number: &str) -> NormalizedOrder {
        NormalizedOrder {
            order_number: Some(number.into()),
            product_names: vec!["商品".into()],
            paid_amount_fen: Some(1999),
            ordered_at: Some("2026-08-13 10:00".into()),
            shop_name: Some("测试店铺".into()),
            warning: None,
            raw_json: "{}".into(),
        }
    }

    #[tokio::test]
    async fn completing_a_pool_task_is_atomic_and_idempotent() {
        let database = Database::in_memory().await.unwrap();
        database
            .create_pool_task("task-1", "order.png", "images/task-1.png", "hash-1")
            .await
            .unwrap();
        assert_eq!(
            database
                .claim_next_realtime_task()
                .await
                .unwrap()
                .unwrap()
                .id,
            "task-1"
        );

        database
            .persist_pool_order("task-1", &normalized("ORDER-1"), "ollama", "vision")
            .await
            .unwrap();
        database
            .persist_pool_order("task-1", &normalized("ORDER-1"), "ollama", "vision")
            .await
            .unwrap();

        assert!(database.pool_task("task-1").await.is_err());
        assert_eq!(
            database
                .list_orders(Default::default())
                .await
                .unwrap()
                .total,
            1
        );
        let logs = database
            .order_import_logs(OrderImportLogsQuery::default())
            .await
            .unwrap();
        assert_eq!(logs.total, 1);
        assert_eq!(logs.items[0].task_id, "task-1");
    }
}
