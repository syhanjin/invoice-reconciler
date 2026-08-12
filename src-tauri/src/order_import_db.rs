use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, FromQueryResult,
    PaginatorTrait, QueryFilter, QueryOrder, Set, Statement, TransactionTrait,
};
use uuid::Uuid;

use crate::db::Database;
use crate::entities::{order, order_import_log, order_import_pool, order_recognition_batch};
use crate::error::{AppError, AppResult};
use crate::models::{
    NormalizedOrder, OrderImportLog, OrderImportLogsPage, OrderImportLogsQuery, OrderImportPool,
    OrderImportTask, OrderRecognitionBatch, OrderRecognitionSettings, Platform,
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
            recognition_batch_id: Set(None),
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
                recognition_batch_id: Set(None),
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
            batch_short_number: Option<String>,
            error_code: Option<String>,
            error_message: Option<String>,
            retry_count: i64,
            created_at: String,
            started_at: Option<String>,
        }
        #[derive(FromQueryResult)]
        struct BatchRow {
            id: String,
            short_number: String,
            status: String,
            model: String,
            task_count: i64,
            error_message: Option<String>,
            created_at: String,
            completed_at: Option<String>,
        }
        let tasks = TaskRow::find_by_statement(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT p.id, p.original_file_name, p.status, b.short_number AS batch_short_number, p.error_code, p.error_message, p.retry_count, p.created_at, p.started_at FROM order_import_pool p LEFT JOIN order_recognition_batches b ON b.id = p.recognition_batch_id ORDER BY p.created_at ASC, p.id ASC",
        ))
        .all(&self.connection)
        .await?
        .into_iter()
        .map(|row| OrderImportTask {
            id: row.id,
            original_file_name: row.original_file_name,
            status: row.status,
            batch_short_number: row.batch_short_number,
            error_code: row.error_code,
            error_message: row.error_message,
            retry_count: row.retry_count,
            created_at: row.created_at,
            started_at: row.started_at,
        })
        .collect::<Vec<_>>();
        let batches = BatchRow::find_by_statement(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT b.id, b.short_number, b.status, b.model, COUNT(p.id) AS task_count, b.error_message, b.created_at, b.completed_at FROM order_recognition_batches b LEFT JOIN order_import_pool p ON p.recognition_batch_id = b.id WHERE b.status NOT IN ('completed', 'failed', 'canceled') GROUP BY b.id ORDER BY b.created_at DESC, b.id DESC",
        ))
        .all(&self.connection)
        .await?
        .into_iter()
        .map(|row| OrderRecognitionBatch {
            id: row.id,
            short_number: row.short_number,
            status: row.status,
            model: row.model,
            task_count: row.task_count,
            error_message: row.error_message,
            created_at: row.created_at,
            completed_at: row.completed_at,
        })
        .collect::<Vec<_>>();
        Ok(OrderImportPool {
            pending: tasks.iter().filter(|task| task.status == "pending").count() as i64,
            processing: tasks
                .iter()
                .filter(|task| {
                    matches!(
                        task.status.as_str(),
                        "processing" | "batch_submitting" | "batch_locked"
                    )
                })
                .count() as i64,
            failed: tasks.iter().filter(|task| task.status == "failed").count() as i64,
            tasks,
            batches,
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
                order_import_pool::Column::RecognitionBatchId,
                Expr::value(Option::<String>::None),
            )
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

    pub async fn fail_recognition_batch_task(
        &self,
        id: &str,
        recognition_batch_id: &str,
        code: &str,
        message: &str,
    ) -> AppResult<bool> {
        let result = order_import_pool::Entity::update_many()
            .col_expr(order_import_pool::Column::Status, Expr::value("failed"))
            .col_expr(
                order_import_pool::Column::RecognitionBatchId,
                Expr::value(Option::<String>::None),
            )
            .col_expr(
                order_import_pool::Column::ErrorCode,
                Expr::value(Some(code.to_owned())),
            )
            .col_expr(
                order_import_pool::Column::ErrorMessage,
                Expr::value(Some(message.to_owned())),
            )
            .filter(order_import_pool::Column::Id.eq(id))
            .filter(order_import_pool::Column::RecognitionBatchId.eq(recognition_batch_id))
            .filter(order_import_pool::Column::Status.is_in(["batch_submitting", "batch_locked"]))
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected == 1)
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
        batch_short_number: Option<&str>,
        recognition_batch_id: Option<&str>,
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
            if recognition_batch_id.is_some() {
                transaction.commit().await?;
                return Ok(false);
            }
            return Err(AppError::NotFound(format!("订单识别任务 {task_id}")));
        };
        let owns_task = match recognition_batch_id {
            Some(batch_id) => {
                task.status == "batch_locked"
                    && task.recognition_batch_id.as_deref() == Some(batch_id)
            }
            None => task.status == "processing" && task.recognition_batch_id.is_none(),
        };
        if !owns_task {
            if recognition_batch_id.is_some() {
                transaction.commit().await?;
                return Ok(false);
            }
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
            batch_short_number: Set(batch_short_number.map(str::to_owned)),
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
            "SELECT task_id, order_id, original_file_name, provider, model, batch_short_number, warning_message, completed_at FROM order_import_logs ORDER BY completed_at DESC, task_id DESC LIMIT ? OFFSET ?",
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

    pub async fn create_recognition_batch(
        &self,
        settings: &OrderRecognitionSettings,
    ) -> AppResult<(
        order_recognition_batch::Model,
        Vec<order_import_pool::Model>,
    )> {
        let transaction = self.connection.begin().await?;
        let tasks = order_import_pool::Entity::find()
            .filter(order_import_pool::Column::Status.eq("pending"))
            .order_by_asc(order_import_pool::Column::CreatedAt)
            .order_by_asc(order_import_pool::Column::Id)
            .all(&transaction)
            .await?;
        if tasks.is_empty() {
            return Err(AppError::InvalidInput("任务池中没有待提交任务".into()));
        }
        if tasks.len() > 50_000 {
            return Err(AppError::InvalidInput(
                "单个百炼批次最多提交 50000 个任务".into(),
            ));
        }
        let next = transaction.query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT COALESCE(MAX(CAST(SUBSTR(short_number, 2) AS INTEGER)), 0) + 1 AS value FROM order_recognition_batches",
        )).await?.and_then(|row| row.try_get::<i64>("", "value").ok()).unwrap_or(1);
        let id = Uuid::now_v7().to_string();
        let model = order_recognition_batch::ActiveModel {
            id: Set(id.clone()),
            short_number: Set(format!("B{next:06}")),
            provider: Set("aliyun_batch".into()),
            base_url: Set(settings.base_url.clone()),
            model: Set(settings.model.clone()),
            enable_thinking: Set(settings.enable_thinking),
            status: Set("uploading".into()),
            input_file_id: Set(None),
            remote_batch_id: Set(None),
            output_file_id: Set(None),
            error_file_id: Set(None),
            error_message: Set(None),
            created_at: Set(now()),
            completed_at: Set(None),
        }
        .insert(&transaction)
        .await?;
        let ids = tasks.iter().map(|task| task.id.clone()).collect::<Vec<_>>();
        let result = order_import_pool::Entity::update_many()
            .col_expr(
                order_import_pool::Column::Status,
                Expr::value("batch_submitting"),
            )
            .col_expr(
                order_import_pool::Column::RecognitionBatchId,
                Expr::value(Some(id)),
            )
            .filter(order_import_pool::Column::Id.is_in(ids))
            .filter(order_import_pool::Column::Status.eq("pending"))
            .exec(&transaction)
            .await?;
        if result.rows_affected != tasks.len() as u64 {
            transaction.rollback().await?;
            return Err(AppError::InvalidInput(
                "任务池已发生变化，请重试提交".into(),
            ));
        }
        transaction.commit().await?;
        Ok((model, tasks))
    }

    pub async fn set_recognition_batch_input_file(
        &self,
        id: &str,
        input_file_id: &str,
    ) -> AppResult<()> {
        let result = order_recognition_batch::Entity::update_many()
            .col_expr(
                order_recognition_batch::Column::InputFileId,
                Expr::value(Some(input_file_id.to_owned())),
            )
            .filter(order_recognition_batch::Column::Id.eq(id))
            .filter(order_recognition_batch::Column::RemoteBatchId.is_null())
            .exec(&self.connection)
            .await?;
        if result.rows_affected != 1 {
            return Err(AppError::NotFound(format!("待恢复识别批次 {id}")));
        }
        Ok(())
    }

    pub async fn mark_recognition_batch_create_pending(&self, id: &str) -> AppResult<()> {
        let result = order_recognition_batch::Entity::update_many()
            .col_expr(
                order_recognition_batch::Column::Status,
                Expr::value("submitted"),
            )
            .filter(order_recognition_batch::Column::Id.eq(id))
            .filter(order_recognition_batch::Column::Status.eq("uploading"))
            .filter(order_recognition_batch::Column::InputFileId.is_not_null())
            .filter(order_recognition_batch::Column::RemoteBatchId.is_null())
            .exec(&self.connection)
            .await?;
        if result.rows_affected != 1 {
            return Err(AppError::InvalidInput(
                "识别批次不处于可创建远端任务的状态".into(),
            ));
        }
        Ok(())
    }

    pub async fn set_recognition_batch_submitted(
        &self,
        id: &str,
        remote_batch_id: &str,
    ) -> AppResult<()> {
        let transaction = self.connection.begin().await?;
        order_recognition_batch::Entity::update_many()
            .col_expr(
                order_recognition_batch::Column::Status,
                Expr::value("submitted"),
            )
            .col_expr(
                order_recognition_batch::Column::RemoteBatchId,
                Expr::value(Some(remote_batch_id.to_owned())),
            )
            .filter(order_recognition_batch::Column::Id.eq(id))
            .exec(&transaction)
            .await?;
        order_import_pool::Entity::update_many()
            .col_expr(
                order_import_pool::Column::Status,
                Expr::value("batch_locked"),
            )
            .filter(order_import_pool::Column::RecognitionBatchId.eq(id))
            .filter(order_import_pool::Column::Status.eq("batch_submitting"))
            .exec(&transaction)
            .await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn recognition_batch(&self, id: &str) -> AppResult<order_recognition_batch::Model> {
        order_recognition_batch::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("识别批次 {id}")))
    }

    pub async fn active_recognition_batches(
        &self,
    ) -> AppResult<Vec<order_recognition_batch::Model>> {
        Ok(order_recognition_batch::Entity::find()
            .filter(order_recognition_batch::Column::Status.is_not_in([
                "completed",
                "failed",
                "canceled",
            ]))
            .all(&self.connection)
            .await?)
    }

    pub async fn update_recognition_batch_remote(
        &self,
        id: &str,
        status: &str,
        output: Option<&str>,
        error: Option<&str>,
    ) -> AppResult<()> {
        let mut update = order_recognition_batch::Entity::update_many().col_expr(
            order_recognition_batch::Column::Status,
            Expr::cust_with_values(
                "CASE WHEN status = 'canceling' AND ? NOT IN ('finalizing', 'canceling') THEN 'canceling' ELSE ? END",
                [status.to_owned(), status.to_owned()],
            ),
        );
        if let Some(output) = output {
            update = update.col_expr(
                order_recognition_batch::Column::OutputFileId,
                Expr::value(Some(output.to_owned())),
            );
        }
        if let Some(error) = error {
            update = update.col_expr(
                order_recognition_batch::Column::ErrorFileId,
                Expr::value(Some(error.to_owned())),
            );
        }
        update
            .filter(order_recognition_batch::Column::Id.eq(id))
            .filter(order_recognition_batch::Column::Status.is_not_in([
                "completed",
                "failed",
                "canceled",
            ]))
            .exec(&self.connection)
            .await?;
        Ok(())
    }

    pub async fn finish_recognition_batch(
        &self,
        id: &str,
        status: &str,
        message: Option<&str>,
    ) -> AppResult<()> {
        let transaction = self.connection.begin().await?;
        order_import_pool::Entity::update_many()
            .col_expr(order_import_pool::Column::Status, Expr::value("failed"))
            .col_expr(
                order_import_pool::Column::RecognitionBatchId,
                Expr::value(Option::<String>::None),
            )
            .col_expr(
                order_import_pool::Column::ErrorCode,
                Expr::value(Some("batch_incomplete".to_owned())),
            )
            .col_expr(
                order_import_pool::Column::ErrorMessage,
                Expr::value(Some(
                    message.unwrap_or("批量识别未返回结果，可重试").to_owned(),
                )),
            )
            .filter(order_import_pool::Column::RecognitionBatchId.eq(id))
            .exec(&transaction)
            .await?;
        order_recognition_batch::Entity::update_many()
            .col_expr(
                order_recognition_batch::Column::Status,
                Expr::value(status.to_owned()),
            )
            .col_expr(
                order_recognition_batch::Column::ErrorMessage,
                Expr::value(message.map(str::to_owned)),
            )
            .col_expr(
                order_recognition_batch::Column::CompletedAt,
                Expr::value(Some(now())),
            )
            .filter(order_recognition_batch::Column::Id.eq(id))
            .exec(&transaction)
            .await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn mark_recognition_batch_canceling(
        &self,
        id: &str,
    ) -> AppResult<order_recognition_batch::Model> {
        let result = order_recognition_batch::Entity::update_many()
            .col_expr(
                order_recognition_batch::Column::Status,
                Expr::value("canceling"),
            )
            .filter(order_recognition_batch::Column::Id.eq(id))
            .filter(order_recognition_batch::Column::Status.is_not_in([
                "completed",
                "failed",
                "canceled",
            ]))
            .exec(&self.connection)
            .await?;
        if result.rows_affected != 1 {
            return Err(AppError::InvalidInput("批次已经结束".into()));
        }
        self.recognition_batch(id).await
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

    fn aliyun_settings() -> OrderRecognitionSettings {
        OrderRecognitionSettings {
            provider: crate::models::OrderRecognitionProvider::AliyunBatch,
            base_url: "https://example.com/v1".into(),
            model: "qwen3-vl-flash".into(),
            api_key: "secret".into(),
            max_concurrency: 3,
            enable_thinking: false,
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
            .persist_pool_order(
                "task-1",
                &normalized("ORDER-1"),
                "ollama",
                "vision",
                None,
                None,
            )
            .await
            .unwrap();
        database
            .persist_pool_order(
                "task-1",
                &normalized("ORDER-1"),
                "ollama",
                "vision",
                None,
                None,
            )
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

    #[tokio::test]
    async fn concurrent_remote_batches_lock_only_their_current_pending_tasks() {
        let database = Database::in_memory().await.unwrap();
        for id in ["task-1", "task-2"] {
            database
                .create_pool_task(id, &format!("{id}.png"), &format!("images/{id}.png"), id)
                .await
                .unwrap();
        }
        let (first, first_tasks) = database
            .create_recognition_batch(&aliyun_settings())
            .await
            .unwrap();
        assert_eq!(first.short_number, "B000001");
        assert_eq!(first_tasks.len(), 2);

        database
            .create_pool_task("task-3", "task-3.png", "images/task-3.png", "task-3")
            .await
            .unwrap();
        let (second, second_tasks) = database
            .create_recognition_batch(&aliyun_settings())
            .await
            .unwrap();
        assert_eq!(second.short_number, "B000002");
        assert_eq!(
            second_tasks
                .iter()
                .map(|task| task.id.as_str())
                .collect::<Vec<_>>(),
            vec!["task-3"]
        );

        assert!(first.id != second.id);
        assert_eq!(
            database
                .pool_task("task-1")
                .await
                .unwrap()
                .recognition_batch_id
                .as_deref(),
            Some(first.id.as_str())
        );
        assert_eq!(
            database
                .pool_task("task-3")
                .await
                .unwrap()
                .recognition_batch_id
                .as_deref(),
            Some(second.id.as_str())
        );

        database
            .set_recognition_batch_input_file(&first.id, "file-1")
            .await
            .unwrap();
        database
            .set_recognition_batch_submitted(&first.id, "remote-1")
            .await
            .unwrap();
        database
            .set_recognition_batch_input_file(&second.id, "file-2")
            .await
            .unwrap();
        database
            .set_recognition_batch_submitted(&second.id, "remote-2")
            .await
            .unwrap();

        assert!(
            !database
                .fail_recognition_batch_task(
                    "task-3",
                    &first.id,
                    "foreign_result",
                    "错误批次返回了该任务",
                )
                .await
                .unwrap()
        );
        assert!(
            !database
                .persist_pool_order(
                    "task-3",
                    &normalized("WRONG-BATCH"),
                    "aliyun_batch",
                    "vision",
                    Some(&first.short_number),
                    Some(&first.id),
                )
                .await
                .unwrap()
        );
        let untouched = database.pool_task("task-3").await.unwrap();
        assert_eq!(untouched.status, "batch_locked");
        assert_eq!(
            untouched.recognition_batch_id.as_deref(),
            Some(second.id.as_str())
        );
        assert_eq!(
            database
                .order_import_logs(OrderImportLogsQuery::default())
                .await
                .unwrap()
                .total,
            0
        );
    }
}
