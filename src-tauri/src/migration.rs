use sea_orm::{ConnectionTrait, Statement};
use sea_orm_migration::prelude::*;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(M0001Initial),
            Box::new(M0002Invoices),
            Box::new(M0003InvoiceRecovery),
            Box::new(M0004OrderInvoiceMatching),
            Box::new(M0005UnifiedOrderImportPool),
            Box::new(M0006RemoveAliyunBatch),
        ]
    }
}

pub struct M0001Initial;

#[async_trait::async_trait]
impl MigrationName for M0001Initial {
    fn name(&self) -> &str {
        "m0001_initial"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for M0001Initial {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_database_backend();
        for sql in UP_STATEMENTS {
            manager
                .get_connection()
                .execute(Statement::from_string(backend, *sql))
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_database_backend();
        for sql in DOWN_STATEMENTS {
            manager
                .get_connection()
                .execute(Statement::from_string(backend, *sql))
                .await?;
        }
        Ok(())
    }
}

const UP_STATEMENTS: &[&str] = &[
    "CREATE TABLE app_settings (id INTEGER PRIMARY KEY CHECK (id = 1), ollama_base_url TEXT NOT NULL, ollama_model TEXT NOT NULL, max_concurrency INTEGER NOT NULL CHECK (max_concurrency BETWEEN 1 AND 8), updated_at TEXT NOT NULL)",
    "INSERT INTO app_settings (id, ollama_base_url, ollama_model, max_concurrency, updated_at) VALUES (1, 'http://127.0.0.1:11434', 'qwen3-vl:4b', 3, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
    "CREATE TABLE import_batches (id TEXT PRIMARY KEY, platform TEXT NOT NULL CHECK (platform IN ('taobao')), status TEXT NOT NULL CHECK (status IN ('queued', 'processing', 'completed', 'completed_with_errors')), created_at TEXT NOT NULL, completed_at TEXT)",
    "CREATE TABLE import_tasks (id TEXT PRIMARY KEY, batch_id TEXT NOT NULL REFERENCES import_batches(id) ON DELETE CASCADE, original_file_name TEXT NOT NULL, managed_image_path TEXT, image_sha256 TEXT NOT NULL, status TEXT NOT NULL CHECK (status IN ('queued', 'processing', 'succeeded', 'failed', 'canceled', 'skipped', 'interrupted')), error_code TEXT, error_message TEXT, warning_message TEXT, retry_count INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL, started_at TEXT, completed_at TEXT)",
    "CREATE UNIQUE INDEX idx_import_tasks_managed_hash ON import_tasks(image_sha256) WHERE managed_image_path IS NOT NULL",
    "CREATE INDEX idx_import_tasks_batch ON import_tasks(batch_id, created_at)",
    "CREATE INDEX idx_import_tasks_status ON import_tasks(status)",
    "CREATE TABLE orders (id TEXT PRIMARY KEY REFERENCES import_tasks(id) ON DELETE CASCADE, platform TEXT NOT NULL CHECK (platform IN ('taobao')), external_order_number TEXT, paid_amount_fen INTEGER, ordered_at TEXT, shop_name TEXT, raw_ocr_json TEXT NOT NULL, possible_duplicate INTEGER NOT NULL DEFAULT 0 CHECK (possible_duplicate IN (0, 1)), created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
    "CREATE INDEX idx_orders_number ON orders(external_order_number)",
    "CREATE INDEX idx_orders_created ON orders(created_at DESC)",
    "CREATE INDEX idx_orders_ordered_at ON orders(ordered_at DESC)",
    "CREATE TABLE order_items (id INTEGER PRIMARY KEY AUTOINCREMENT, order_id TEXT NOT NULL REFERENCES orders(id) ON DELETE CASCADE, position INTEGER NOT NULL, product_name TEXT NOT NULL, UNIQUE(order_id, position))",
    "CREATE INDEX idx_order_items_order ON order_items(order_id, position)",
];

const DOWN_STATEMENTS: &[&str] = &[
    "DROP TABLE IF EXISTS order_items",
    "DROP TABLE IF EXISTS orders",
    "DROP TABLE IF EXISTS import_tasks",
    "DROP TABLE IF EXISTS import_batches",
    "DROP TABLE IF EXISTS app_settings",
];

pub struct M0002Invoices;

#[async_trait::async_trait]
impl MigrationName for M0002Invoices {
    fn name(&self) -> &str {
        "m0002_invoices"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for M0002Invoices {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_database_backend();
        for sql in INVOICE_UP_STATEMENTS {
            manager
                .get_connection()
                .execute(Statement::from_string(backend, *sql))
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_database_backend();
        for sql in INVOICE_DOWN_STATEMENTS {
            manager
                .get_connection()
                .execute(Statement::from_string(backend, *sql))
                .await?;
        }
        Ok(())
    }
}

const INVOICE_UP_STATEMENTS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS invoice_batches (id TEXT PRIMARY KEY, status TEXT NOT NULL CHECK (status IN ('queued', 'processing', 'awaiting_review', 'completed', 'completed_with_errors')), forced_type INTEGER, created_at TEXT NOT NULL, completed_at TEXT)",
    "CREATE TABLE IF NOT EXISTS invoice_tasks (id TEXT PRIMARY KEY, batch_id TEXT NOT NULL REFERENCES invoice_batches(id) ON DELETE CASCADE, original_file_name TEXT NOT NULL, source_path TEXT, source_sha256 TEXT NOT NULL, media_type TEXT NOT NULL, forced_type INTEGER, status TEXT NOT NULL CHECK (status IN ('queued', 'processing', 'awaiting_review', 'succeeded', 'failed', 'canceled', 'interrupted')), error_code TEXT, error_message TEXT, warning_message TEXT, retry_count INTEGER NOT NULL DEFAULT 0, request_id TEXT, raw_response_json TEXT, existing_invoice_id TEXT, created_at TEXT NOT NULL, started_at TEXT, completed_at TEXT)",
    "CREATE INDEX IF NOT EXISTS idx_invoice_tasks_batch ON invoice_tasks(batch_id, created_at, id)",
    "CREATE INDEX IF NOT EXISTS idx_invoice_tasks_hash ON invoice_tasks(source_sha256)",
    "CREATE INDEX IF NOT EXISTS idx_invoice_tasks_status ON invoice_tasks(status)",
    "CREATE TABLE IF NOT EXISTS invoices (id TEXT PRIMARY KEY, task_id TEXT NOT NULL REFERENCES invoice_tasks(id) ON DELETE CASCADE, type INTEGER NOT NULL, subtype TEXT NOT NULL, type_description TEXT NOT NULL, subtype_description TEXT NOT NULL, invoice_code TEXT, invoice_number TEXT, invoice_date TEXT, buyer_name TEXT, buyer_tax_id TEXT, seller_name TEXT, seller_tax_id TEXT, amount_fen INTEGER, tax_fen INTEGER, total_fen INTEGER, currency TEXT NOT NULL DEFAULT 'CNY', reliable_identity TEXT, possible_duplicate INTEGER NOT NULL DEFAULT 0 CHECK (possible_duplicate IN (0, 1)), preview_path TEXT NOT NULL, page_start INTEGER NOT NULL DEFAULT 1, page_end INTEGER NOT NULL DEFAULT 1, raw_invoice_json TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
    "CREATE UNIQUE INDEX IF NOT EXISTS idx_invoices_reliable_identity ON invoices(reliable_identity) WHERE reliable_identity IS NOT NULL",
    "CREATE INDEX IF NOT EXISTS idx_invoices_date ON invoices(invoice_date DESC, id DESC)",
    "CREATE INDEX IF NOT EXISTS idx_invoices_created ON invoices(created_at DESC, id DESC)",
    "CREATE INDEX IF NOT EXISTS idx_invoices_subtype ON invoices(subtype)",
    "CREATE TABLE IF NOT EXISTS invoice_items (id INTEGER PRIMARY KEY AUTOINCREMENT, invoice_id TEXT NOT NULL REFERENCES invoices(id) ON DELETE CASCADE, position INTEGER NOT NULL, name TEXT NOT NULL, specification TEXT, unit TEXT, quantity TEXT, unit_price_fen INTEGER, amount_fen INTEGER, tax_rate TEXT, tax_fen INTEGER, raw_json TEXT NOT NULL, UNIQUE(invoice_id, position))",
    "CREATE INDEX IF NOT EXISTS idx_invoice_items_invoice ON invoice_items(invoice_id, position)",
    "CREATE TABLE IF NOT EXISTS invoice_source_links (id INTEGER PRIMARY KEY AUTOINCREMENT, invoice_id TEXT NOT NULL REFERENCES invoices(id) ON DELETE CASCADE, task_id TEXT NOT NULL REFERENCES invoice_tasks(id) ON DELETE CASCADE, page_start INTEGER NOT NULL, page_end INTEGER NOT NULL, UNIQUE(invoice_id, task_id, page_start, page_end))",
    "CREATE INDEX IF NOT EXISTS idx_invoice_sources_task ON invoice_source_links(task_id)",
];

const INVOICE_DOWN_STATEMENTS: &[&str] = &[
    "DROP TABLE IF EXISTS invoice_source_links",
    "DROP TABLE IF EXISTS invoice_items",
    "DROP TABLE IF EXISTS invoices",
    "DROP TABLE IF EXISTS invoice_tasks",
    "DROP TABLE IF EXISTS invoice_batches",
];

pub struct M0003InvoiceRecovery;

#[async_trait::async_trait]
impl MigrationName for M0003InvoiceRecovery {
    fn name(&self) -> &str {
        "m0003_invoice_recovery"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for M0003InvoiceRecovery {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_database_backend();
        for sql in INVOICE_RECOVERY_UP_STATEMENTS {
            manager
                .get_connection()
                .execute(Statement::from_string(backend, *sql))
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_database_backend();
        for sql in INVOICE_RECOVERY_DOWN_STATEMENTS {
            manager
                .get_connection()
                .execute(Statement::from_string(backend, *sql))
                .await?;
        }
        Ok(())
    }
}

const INVOICE_RECOVERY_UP_STATEMENTS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS invoice_recovery_markers (source_sha256 TEXT NOT NULL, recovery_identity TEXT NOT NULL, recovery_task_id TEXT REFERENCES invoice_tasks(id) ON DELETE SET NULL, deleted_at TEXT NOT NULL, PRIMARY KEY (source_sha256, recovery_identity))",
    "CREATE INDEX IF NOT EXISTS idx_invoice_recovery_task ON invoice_recovery_markers(recovery_task_id)",
];

const INVOICE_RECOVERY_DOWN_STATEMENTS: &[&str] =
    &["DROP TABLE IF EXISTS invoice_recovery_markers"];

pub struct M0004OrderInvoiceMatching;

#[async_trait::async_trait]
impl MigrationName for M0004OrderInvoiceMatching {
    fn name(&self) -> &str {
        "m0004_order_invoice_matching"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for M0004OrderInvoiceMatching {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_database_backend();
        for sql in MATCHING_UP_STATEMENTS {
            manager
                .get_connection()
                .execute(Statement::from_string(backend, *sql))
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_database_backend();
        for sql in MATCHING_DOWN_STATEMENTS {
            manager
                .get_connection()
                .execute(Statement::from_string(backend, *sql))
                .await?;
        }
        Ok(())
    }
}

const MATCHING_UP_STATEMENTS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS invoice_matches (invoice_id TEXT PRIMARY KEY REFERENCES invoices(id) ON DELETE CASCADE, source TEXT NOT NULL CHECK (source IN ('automatic', 'custom')), confirmed_at TEXT NOT NULL)",
    "CREATE INDEX IF NOT EXISTS idx_invoice_matches_confirmed ON invoice_matches(confirmed_at DESC, invoice_id)",
    "CREATE TABLE IF NOT EXISTS invoice_match_orders (invoice_id TEXT NOT NULL REFERENCES invoice_matches(invoice_id) ON DELETE CASCADE, order_id TEXT NOT NULL UNIQUE REFERENCES orders(id) ON DELETE CASCADE, position INTEGER NOT NULL, PRIMARY KEY (invoice_id, order_id), UNIQUE (invoice_id, position))",
    "CREATE INDEX IF NOT EXISTS idx_invoice_match_orders_invoice ON invoice_match_orders(invoice_id, position)",
];

const MATCHING_DOWN_STATEMENTS: &[&str] = &[
    "DROP TABLE IF EXISTS invoice_match_orders",
    "DROP TABLE IF EXISTS invoice_matches",
];

pub struct M0005UnifiedOrderImportPool;

#[async_trait::async_trait]
impl MigrationName for M0005UnifiedOrderImportPool {
    fn name(&self) -> &str {
        "m0005_unified_order_import_pool"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for M0005UnifiedOrderImportPool {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_database_backend();
        let connection = manager.get_connection();

        // SQLite cannot add several columns conditionally in one statement. Checking
        // each column keeps an interrupted migration resumable.
        for (column, sql) in ORDER_IMAGE_COLUMNS {
            if !manager.has_column("orders", column).await? {
                connection
                    .execute(Statement::from_string(backend, *sql))
                    .await?;
            }
        }
        for sql in ORDER_POOL_UP_STATEMENTS {
            connection
                .execute(Statement::from_string(backend, *sql))
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_database_backend();
        for sql in ORDER_POOL_DOWN_STATEMENTS {
            manager
                .get_connection()
                .execute(Statement::from_string(backend, *sql))
                .await?;
        }
        Ok(())
    }
}

const ORDER_IMAGE_COLUMNS: &[(&str, &str)] = &[
    (
        "original_file_name",
        "ALTER TABLE orders ADD COLUMN original_file_name TEXT",
    ),
    (
        "managed_image_path",
        "ALTER TABLE orders ADD COLUMN managed_image_path TEXT",
    ),
    (
        "image_sha256",
        "ALTER TABLE orders ADD COLUMN image_sha256 TEXT",
    ),
    (
        "warning_message",
        "ALTER TABLE orders ADD COLUMN warning_message TEXT",
    ),
];

const ORDER_POOL_UP_STATEMENTS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS order_recognition_batches (id TEXT PRIMARY KEY, short_number TEXT NOT NULL UNIQUE, provider TEXT NOT NULL CHECK (provider = 'aliyun_batch'), base_url TEXT NOT NULL, model TEXT NOT NULL, enable_thinking INTEGER NOT NULL DEFAULT 0 CHECK (enable_thinking IN (0, 1)), status TEXT NOT NULL CHECK (status IN ('uploading', 'submitted', 'validating', 'in_progress', 'finalizing', 'completed', 'failed', 'canceling', 'canceled')), input_file_id TEXT, remote_batch_id TEXT UNIQUE, output_file_id TEXT, error_file_id TEXT, error_message TEXT, created_at TEXT NOT NULL, completed_at TEXT)",
    "CREATE INDEX IF NOT EXISTS idx_order_recognition_batches_status ON order_recognition_batches(status, created_at)",
    "CREATE TABLE IF NOT EXISTS order_import_pool (id TEXT PRIMARY KEY, original_file_name TEXT NOT NULL, managed_image_path TEXT NOT NULL, image_sha256 TEXT NOT NULL, status TEXT NOT NULL CHECK (status IN ('pending', 'processing', 'batch_submitting', 'batch_locked', 'failed')), recognition_batch_id TEXT REFERENCES order_recognition_batches(id) ON DELETE SET NULL, error_code TEXT, error_message TEXT, retry_count INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL, started_at TEXT)",
    "CREATE UNIQUE INDEX IF NOT EXISTS idx_order_import_pool_hash ON order_import_pool(image_sha256)",
    "CREATE INDEX IF NOT EXISTS idx_order_import_pool_status ON order_import_pool(status, created_at, id)",
    "CREATE INDEX IF NOT EXISTS idx_order_import_pool_batch ON order_import_pool(recognition_batch_id, created_at, id)",
    "CREATE TABLE IF NOT EXISTS order_import_logs (task_id TEXT PRIMARY KEY, order_id TEXT NOT NULL, original_file_name TEXT NOT NULL, provider TEXT NOT NULL, model TEXT NOT NULL, batch_short_number TEXT, warning_message TEXT, completed_at TEXT NOT NULL)",
    "CREATE INDEX IF NOT EXISTS idx_order_import_logs_completed ON order_import_logs(completed_at DESC, task_id DESC)",
    "CREATE UNIQUE INDEX IF NOT EXISTS idx_orders_managed_hash ON orders(image_sha256) WHERE managed_image_path IS NOT NULL",
    "UPDATE orders SET original_file_name = COALESCE(original_file_name, (SELECT original_file_name FROM import_tasks WHERE import_tasks.id = orders.id)), managed_image_path = COALESCE(managed_image_path, (SELECT managed_image_path FROM import_tasks WHERE import_tasks.id = orders.id)), image_sha256 = COALESCE(image_sha256, (SELECT image_sha256 FROM import_tasks WHERE import_tasks.id = orders.id)), warning_message = COALESCE(warning_message, (SELECT warning_message FROM import_tasks WHERE import_tasks.id = orders.id))",
    "INSERT OR IGNORE INTO order_import_logs (task_id, order_id, original_file_name, provider, model, warning_message, completed_at) SELECT t.id, o.id, t.original_file_name, 'ollama', 'legacy', t.warning_message, COALESCE(t.completed_at, o.created_at) FROM import_tasks t JOIN orders o ON o.id = t.id",
    "INSERT OR IGNORE INTO order_import_pool (id, original_file_name, managed_image_path, image_sha256, status, error_code, error_message, retry_count, created_at, started_at) SELECT t.id, t.original_file_name, t.managed_image_path, t.image_sha256, 'failed', CASE WHEN t.status IN ('queued', 'processing') THEN 'app_interrupted' ELSE COALESCE(t.error_code, 'legacy_incomplete') END, CASE WHEN t.status IN ('queued', 'processing') THEN '应用在识别完成前退出，可手动重试' ELSE COALESCE(t.error_message, '旧版未完成任务，可手动重试') END, t.retry_count, t.created_at, t.started_at FROM import_tasks t LEFT JOIN orders o ON o.id = t.id WHERE o.id IS NULL AND t.managed_image_path IS NOT NULL AND t.status IN ('queued', 'processing', 'failed', 'interrupted')",
];

const ORDER_POOL_DOWN_STATEMENTS: &[&str] = &[
    "DROP TABLE IF EXISTS order_import_logs",
    "DROP TABLE IF EXISTS order_import_pool",
    "DROP TABLE IF EXISTS order_recognition_batches",
];

pub struct M0006RemoveAliyunBatch;

#[async_trait::async_trait]
impl MigrationName for M0006RemoveAliyunBatch {
    fn name(&self) -> &str {
        "m0006_remove_aliyun_batch"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for M0006RemoveAliyunBatch {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_database_backend();
        let connection = manager.get_connection();
        let pool_exists = manager.has_table("order_import_pool").await?;
        let pool_needs_rebuild = pool_exists
            && manager
                .has_column("order_import_pool", "recognition_batch_id")
                .await?;
        let pool_backup_exists = manager.has_table("order_import_pool_m0006_old").await?;
        if pool_needs_rebuild {
            connection
                .execute(Statement::from_string(
                    backend,
                    "DROP TABLE IF EXISTS order_import_pool_m0006_old",
                ))
                .await?;
            connection
                .execute(Statement::from_string(
                    backend,
                    "ALTER TABLE order_import_pool RENAME TO order_import_pool_m0006_old",
                ))
                .await?;
        }
        connection
            .execute(Statement::from_string(
                backend,
                "CREATE TABLE IF NOT EXISTS order_import_pool (id TEXT PRIMARY KEY, original_file_name TEXT NOT NULL, managed_image_path TEXT NOT NULL, image_sha256 TEXT NOT NULL, status TEXT NOT NULL CHECK (status IN ('pending', 'processing', 'failed')), error_code TEXT, error_message TEXT, retry_count INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL, started_at TEXT)",
            ))
            .await?;
        if pool_needs_rebuild || pool_backup_exists {
            connection
                .execute(Statement::from_string(
                    backend,
                    "INSERT OR IGNORE INTO order_import_pool (id, original_file_name, managed_image_path, image_sha256, status, error_code, error_message, retry_count, created_at, started_at) SELECT id, original_file_name, managed_image_path, image_sha256, CASE WHEN status IN ('batch_submitting', 'batch_locked') THEN 'failed' ELSE status END, CASE WHEN status IN ('batch_submitting', 'batch_locked') THEN 'batch_api_removed' ELSE error_code END, CASE WHEN status IN ('batch_submitting', 'batch_locked') THEN '阿里云 Batch 功能已移除，请配置实时识别服务后手动重试' ELSE error_message END, retry_count, created_at, started_at FROM order_import_pool_m0006_old",
                ))
                .await?;
            connection
                .execute(Statement::from_string(
                    backend,
                    "DROP TABLE order_import_pool_m0006_old",
                ))
                .await?;
        }

        let logs_exists = manager.has_table("order_import_logs").await?;
        let logs_need_rebuild = logs_exists
            && manager
                .has_column("order_import_logs", "batch_short_number")
                .await?;
        let logs_backup_exists = manager.has_table("order_import_logs_m0006_old").await?;
        if logs_need_rebuild {
            connection
                .execute(Statement::from_string(
                    backend,
                    "DROP TABLE IF EXISTS order_import_logs_m0006_old",
                ))
                .await?;
            connection
                .execute(Statement::from_string(
                    backend,
                    "ALTER TABLE order_import_logs RENAME TO order_import_logs_m0006_old",
                ))
                .await?;
        }
        connection
            .execute(Statement::from_string(
                backend,
                "CREATE TABLE IF NOT EXISTS order_import_logs (task_id TEXT PRIMARY KEY, order_id TEXT NOT NULL, original_file_name TEXT NOT NULL, provider TEXT NOT NULL, model TEXT NOT NULL, warning_message TEXT, completed_at TEXT NOT NULL)",
            ))
            .await?;
        if logs_need_rebuild || logs_backup_exists {
            connection
                .execute(Statement::from_string(
                    backend,
                    "INSERT OR IGNORE INTO order_import_logs (task_id, order_id, original_file_name, provider, model, warning_message, completed_at) SELECT task_id, order_id, original_file_name, provider, model, warning_message, completed_at FROM order_import_logs_m0006_old",
                ))
                .await?;
            connection
                .execute(Statement::from_string(
                    backend,
                    "DROP TABLE order_import_logs_m0006_old",
                ))
                .await?;
        }

        for sql in [
            "DROP TABLE IF EXISTS order_recognition_batches",
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_order_import_pool_hash ON order_import_pool(image_sha256)",
            "CREATE INDEX IF NOT EXISTS idx_order_import_pool_status ON order_import_pool(status, created_at, id)",
            "CREATE INDEX IF NOT EXISTS idx_order_import_logs_completed ON order_import_logs(completed_at DESC, task_id DESC)",
        ] {
            connection
                .execute(Statement::from_string(backend, sql))
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_database_backend();
        let connection = manager.get_connection();
        for sql in [
            ORDER_POOL_UP_STATEMENTS[0],
            "CREATE TABLE IF NOT EXISTS order_import_pool_m0006_down (id TEXT PRIMARY KEY, original_file_name TEXT NOT NULL, managed_image_path TEXT NOT NULL, image_sha256 TEXT NOT NULL, status TEXT NOT NULL CHECK (status IN ('pending', 'processing', 'batch_submitting', 'batch_locked', 'failed')), recognition_batch_id TEXT REFERENCES order_recognition_batches(id) ON DELETE SET NULL, error_code TEXT, error_message TEXT, retry_count INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL, started_at TEXT)",
            "INSERT OR IGNORE INTO order_import_pool_m0006_down (id, original_file_name, managed_image_path, image_sha256, status, error_code, error_message, retry_count, created_at, started_at) SELECT id, original_file_name, managed_image_path, image_sha256, status, error_code, error_message, retry_count, created_at, started_at FROM order_import_pool",
            "DROP TABLE order_import_pool",
            "ALTER TABLE order_import_pool_m0006_down RENAME TO order_import_pool",
            "CREATE TABLE IF NOT EXISTS order_import_logs_m0006_down (task_id TEXT PRIMARY KEY, order_id TEXT NOT NULL, original_file_name TEXT NOT NULL, provider TEXT NOT NULL, model TEXT NOT NULL, batch_short_number TEXT, warning_message TEXT, completed_at TEXT NOT NULL)",
            "INSERT OR IGNORE INTO order_import_logs_m0006_down (task_id, order_id, original_file_name, provider, model, warning_message, completed_at) SELECT task_id, order_id, original_file_name, provider, model, warning_message, completed_at FROM order_import_logs",
            "DROP TABLE order_import_logs",
            "ALTER TABLE order_import_logs_m0006_down RENAME TO order_import_logs",
            ORDER_POOL_UP_STATEMENTS[1],
            ORDER_POOL_UP_STATEMENTS[3],
            ORDER_POOL_UP_STATEMENTS[4],
            ORDER_POOL_UP_STATEMENTS[5],
            ORDER_POOL_UP_STATEMENTS[7],
        ] {
            connection
                .execute(Statement::from_string(backend, sql))
                .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use sea_orm::{ConnectOptions, ConnectionTrait, DbBackend, Statement, TryGetable};
    use sea_orm_migration::MigratorTrait;

    use super::Migrator;
    use crate::db::Database;

    async fn connect_raw(path: &std::path::Path) -> sea_orm::DatabaseConnection {
        let mut options = ConnectOptions::new(format!("sqlite://{}?mode=rwc", path.display()));
        options.max_connections(1).sqlx_logging(false);
        sea_orm::Database::connect(options).await.unwrap()
    }

    async fn scalar(connection: &sea_orm::DatabaseConnection, sql: &str) -> i64 {
        let row = connection
            .query_one(Statement::from_string(DbBackend::Sqlite, sql))
            .await
            .unwrap()
            .unwrap();
        i64::try_get(&row, "", "count").unwrap()
    }

    #[tokio::test]
    async fn fresh_file_database_applies_all_migrations() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fresh.sqlite3");
        let database = Database::connect(&path).await.unwrap();

        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM seaql_migrations WHERE version IN ('m0001_initial', 'm0002_invoices', 'm0003_invoice_recovery', 'm0004_order_invoice_matching', 'm0005_unified_order_import_pool', 'm0006_remove_aliyun_batch')",
            )
            .await,
            6
        );
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM sqlite_master WHERE type = 'table' AND name IN ('invoice_batches', 'invoice_tasks', 'invoices', 'invoice_items', 'invoice_source_links', 'invoice_recovery_markers', 'invoice_matches', 'invoice_match_orders')",
            )
            .await,
            8
        );
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM sqlite_master WHERE type = 'index' AND name = 'idx_invoices_reliable_identity'",
            )
            .await,
            1
        );
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM sqlite_master WHERE type = 'table' AND name IN ('order_import_pool', 'order_import_logs')",
            )
            .await,
            2
        );
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM sqlite_master WHERE type = 'table' AND name = 'order_recognition_batches'",
            )
            .await,
            0
        );
    }

    #[tokio::test]
    async fn upgrades_m0001_database_without_losing_orders() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("upgrade.sqlite3");
        let connection = connect_raw(&path).await;
        Migrator::up(&connection, Some(1)).await.unwrap();
        connection.execute_unprepared(
            "INSERT INTO import_batches (id, platform, status, created_at) VALUES ('legacy-batch', 'taobao', 'queued', '2026-08-12T00:00:00Z');
             INSERT INTO import_tasks (id, batch_id, original_file_name, managed_image_path, image_sha256, status, retry_count, created_at) VALUES ('legacy-task', 'legacy-batch', 'order.png', 'images/order.png', 'legacy-hash', 'queued', 0, '2026-08-12T00:00:00Z');
             INSERT INTO orders (id, platform, external_order_number, paid_amount_fen, raw_ocr_json, possible_duplicate, created_at, updated_at) VALUES ('legacy-task', 'taobao', 'ORDER-001', 12345, '{}', 0, '2026-08-12T00:00:00Z', '2026-08-12T00:00:00Z');
             INSERT INTO order_items (order_id, position, product_name) VALUES ('legacy-task', 0, '保留商品');"
        ).await.unwrap();
        connection.close().await.unwrap();

        let database = Database::connect(&path).await.unwrap();
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM orders WHERE external_order_number = 'ORDER-001' AND paid_amount_fen = 12345",
            )
            .await,
            1
        );
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM orders WHERE id = 'legacy-task' AND original_file_name = 'order.png' AND managed_image_path = 'images/order.png' AND image_sha256 = 'legacy-hash'",
            )
            .await,
            1
        );
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM order_import_logs WHERE task_id = 'legacy-task' AND order_id = 'legacy-task' AND provider = 'ollama' AND model = 'legacy'",
            )
            .await,
            1
        );
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM order_import_pool WHERE id = 'legacy-task'",
            )
            .await,
            0
        );
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM order_items WHERE product_name = '保留商品'",
            )
            .await,
            1
        );
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM import_tasks WHERE id = 'legacy-task' AND status = 'interrupted' AND error_code = 'app_interrupted'",
            )
            .await,
            1
        );
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM seaql_migrations WHERE version = 'm0002_invoices'",
            )
            .await,
            1
        );
    }

    #[tokio::test]
    async fn migrates_legacy_incomplete_orders_into_failed_unified_pool() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("upgrade-pool.sqlite3");
        let connection = connect_raw(&path).await;
        Migrator::up(&connection, Some(4)).await.unwrap();
        connection.execute_unprepared(
            "INSERT INTO import_batches (id, platform, status, created_at) VALUES ('legacy-batch', 'taobao', 'processing', '2026-08-12T00:00:00Z');
             INSERT INTO import_tasks (id, batch_id, original_file_name, managed_image_path, image_sha256, status, retry_count, created_at) VALUES ('legacy-pending', 'legacy-batch', 'pending.png', 'images/pending.png', 'pending-hash', 'processing', 2, '2026-08-12T00:00:00Z');"
        ).await.unwrap();
        connection.close().await.unwrap();

        let database = Database::connect(&path).await.unwrap();
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM order_import_pool WHERE id = 'legacy-pending' AND status = 'failed' AND error_code = 'app_interrupted' AND retry_count = 2",
            )
            .await,
            1
        );
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM order_import_logs WHERE task_id = 'legacy-pending'",
            )
            .await,
            0
        );
    }

    #[tokio::test]
    async fn resumes_an_interrupted_unified_pool_migration() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("resume-pool.sqlite3");
        let connection = connect_raw(&path).await;
        Migrator::up(&connection, Some(4)).await.unwrap();
        connection.execute_unprepared(
            "ALTER TABLE orders ADD COLUMN original_file_name TEXT;
             CREATE TABLE order_recognition_batches (id TEXT PRIMARY KEY, short_number TEXT NOT NULL UNIQUE, provider TEXT NOT NULL CHECK (provider = 'aliyun_batch'), base_url TEXT NOT NULL, model TEXT NOT NULL, enable_thinking INTEGER NOT NULL DEFAULT 0 CHECK (enable_thinking IN (0, 1)), status TEXT NOT NULL CHECK (status IN ('uploading', 'submitted', 'validating', 'in_progress', 'finalizing', 'completed', 'failed', 'canceling', 'canceled')), input_file_id TEXT, remote_batch_id TEXT UNIQUE, output_file_id TEXT, error_file_id TEXT, error_message TEXT, created_at TEXT NOT NULL, completed_at TEXT);"
        ).await.unwrap();
        connection.close().await.unwrap();

        let database = Database::connect(&path).await.unwrap();
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM sqlite_master WHERE type = 'table' AND name IN ('order_import_pool', 'order_import_logs')",
            )
            .await,
            2
        );
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM seaql_migrations WHERE version = 'm0005_unified_order_import_pool'",
            )
            .await,
            1
        );
    }

    #[tokio::test]
    async fn resumes_an_interrupted_invoice_migration() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("resume.sqlite3");
        let connection = connect_raw(&path).await;
        Migrator::up(&connection, Some(1)).await.unwrap();
        connection
            .execute_unprepared(
                "CREATE TABLE invoice_batches (id TEXT PRIMARY KEY, status TEXT NOT NULL CHECK (status IN ('queued', 'processing', 'awaiting_review', 'completed', 'completed_with_errors')), forced_type INTEGER, created_at TEXT NOT NULL, completed_at TEXT)",
            )
            .await
            .unwrap();
        connection.close().await.unwrap();

        let database = Database::connect(&path).await.unwrap();
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM sqlite_master WHERE type = 'table' AND name IN ('invoice_batches', 'invoice_tasks', 'invoices', 'invoice_items', 'invoice_source_links')",
            )
            .await,
            5
        );
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM seaql_migrations WHERE version = 'm0002_invoices'",
            )
            .await,
            1
        );
    }

    #[tokio::test]
    async fn resumes_an_interrupted_invoice_recovery_migration() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("resume-recovery.sqlite3");
        let connection = connect_raw(&path).await;
        Migrator::up(&connection, Some(2)).await.unwrap();
        connection
            .execute_unprepared(
                "CREATE TABLE invoice_recovery_markers (source_sha256 TEXT NOT NULL, recovery_identity TEXT NOT NULL, recovery_task_id TEXT REFERENCES invoice_tasks(id) ON DELETE SET NULL, deleted_at TEXT NOT NULL, PRIMARY KEY (source_sha256, recovery_identity))",
            )
            .await
            .unwrap();
        connection.close().await.unwrap();

        let database = Database::connect(&path).await.unwrap();
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM sqlite_master WHERE type = 'table' AND name = 'invoice_recovery_markers'",
            )
            .await,
            1
        );
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM seaql_migrations WHERE version = 'm0003_invoice_recovery'",
            )
            .await,
            1
        );
    }

    #[tokio::test]
    async fn upgrades_m0002_database_without_losing_invoices() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("upgrade-recovery.sqlite3");
        let connection = connect_raw(&path).await;
        Migrator::up(&connection, Some(2)).await.unwrap();
        connection.execute_unprepared(
            "PRAGMA foreign_keys = ON;
             INSERT INTO invoice_batches (id, status, created_at) VALUES ('legacy-batch', 'completed', '2026-08-12T00:00:00Z');
             INSERT INTO invoice_tasks (id, batch_id, original_file_name, source_path, source_sha256, media_type, status, retry_count, created_at) VALUES ('legacy-task', 'legacy-batch', 'legacy.pdf', 'invoices/sources/legacy.pdf', 'legacy-hash', 'application/pdf', 'succeeded', 0, '2026-08-12T00:00:00Z');
             INSERT INTO invoices (id, task_id, type, subtype, type_description, subtype_description, invoice_number, reliable_identity, currency, preview_path, raw_invoice_json, created_at, updated_at) VALUES ('legacy-invoice', 'legacy-task', 3, 'VatSpecialInvoice', '增值税发票', '增值税专用发票', '1001', 'vat|4403|1001', 'CNY', 'invoices/previews/legacy.pdf', '{}', '2026-08-12T00:00:00Z', '2026-08-12T00:00:00Z');"
        ).await.unwrap();
        connection.close().await.unwrap();

        let database = Database::connect(&path).await.unwrap();
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM invoices WHERE id = 'legacy-invoice' AND task_id = 'legacy-task'",
            )
            .await,
            1
        );
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM sqlite_master WHERE type = 'table' AND name = 'invoice_recovery_markers'",
            )
            .await,
            1
        );
    }

    #[tokio::test]
    async fn restart_interrupts_invoice_work_only_after_schema_is_available() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("restart.sqlite3");
        let connection = connect_raw(&path).await;
        Migrator::up(&connection, None).await.unwrap();
        connection.execute_unprepared(
            "INSERT INTO invoice_batches (id, status, created_at) VALUES ('batch', 'processing', '2026-08-12T00:00:00Z');
             INSERT INTO invoice_tasks (id, batch_id, original_file_name, source_sha256, media_type, status, retry_count, created_at) VALUES ('queued-task', 'batch', 'queued.pdf', 'queued-hash', 'application/pdf', 'queued', 0, '2026-08-12T00:00:00Z');
             INSERT INTO invoice_tasks (id, batch_id, original_file_name, source_sha256, media_type, status, retry_count, created_at) VALUES ('processing-task', 'batch', 'processing.pdf', 'processing-hash', 'application/pdf', 'processing', 0, '2026-08-12T00:00:00Z');"
        ).await.unwrap();
        connection.close().await.unwrap();

        let database = Database::connect(&path).await.unwrap();
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM invoice_tasks WHERE status = 'interrupted' AND error_code = 'app_interrupted' AND completed_at IS NOT NULL",
            )
            .await,
            2
        );
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM invoice_batches WHERE id = 'batch' AND status = 'completed_with_errors' AND completed_at IS NOT NULL",
            )
            .await,
            1
        );
    }

    #[tokio::test]
    async fn upgrade_releases_removed_batch_work_as_failed_tasks() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("restart-orders.sqlite3");
        let connection = connect_raw(&path).await;
        Migrator::up(&connection, Some(5)).await.unwrap();
        connection.execute_unprepared(
            "INSERT INTO order_recognition_batches (id, short_number, provider, base_url, model, status, remote_batch_id, created_at) VALUES ('batch', 'B000001', 'aliyun_batch', 'https://example.com/v1', 'qwen', 'in_progress', 'remote-batch', '2026-08-13T00:00:00Z');
             INSERT INTO order_import_pool (id, original_file_name, managed_image_path, image_sha256, status, retry_count, created_at) VALUES ('realtime', 'realtime.png', 'images/realtime.png', 'realtime-hash', 'processing', 0, '2026-08-13T00:00:00Z');
             INSERT INTO order_import_pool (id, original_file_name, managed_image_path, image_sha256, status, recognition_batch_id, retry_count, created_at) VALUES ('remote', 'remote.png', 'images/remote.png', 'remote-hash', 'batch_locked', 'batch', 0, '2026-08-13T00:00:00Z');"
        ).await.unwrap();
        connection.close().await.unwrap();

        let database = Database::connect(&path).await.unwrap();
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM order_import_pool WHERE id = 'realtime' AND status = 'failed' AND error_code = 'app_interrupted'",
            )
            .await,
            1
        );
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM order_import_pool WHERE id = 'remote' AND status = 'failed' AND error_code = 'batch_api_removed' AND error_message LIKE '%手动重试%'",
            )
            .await,
            1
        );
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM sqlite_master WHERE type = 'table' AND name = 'order_recognition_batches'",
            )
            .await,
            0
        );
    }

    #[tokio::test]
    async fn upgrade_preserves_historical_aliyun_orders_and_audit_source() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("historical-aliyun.sqlite3");
        let connection = connect_raw(&path).await;
        Migrator::up(&connection, Some(5)).await.unwrap();
        connection.execute_unprepared(
            "INSERT INTO import_batches (id, platform, status, created_at, completed_at) VALUES ('legacy-pool', 'taobao', 'completed', '2026-08-13T00:00:00Z', '2026-08-13T00:01:00Z');
             INSERT INTO import_tasks (id, batch_id, original_file_name, image_sha256, status, retry_count, created_at, completed_at) VALUES ('order-a', 'legacy-pool', 'order.png', 'order-hash', 'succeeded', 0, '2026-08-13T00:00:00Z', '2026-08-13T00:01:00Z');
             INSERT INTO orders (id, platform, external_order_number, raw_ocr_json, possible_duplicate, created_at, updated_at, original_file_name, managed_image_path, image_sha256) VALUES ('order-a', 'taobao', 'ORDER-A', '{}', 0, '2026-08-13T00:01:00Z', '2026-08-13T00:01:00Z', 'order.png', 'images/order-a.png', 'order-hash');
             INSERT INTO order_import_logs (task_id, order_id, original_file_name, provider, model, batch_short_number, completed_at) VALUES ('order-a', 'order-a', 'order.png', 'aliyun_batch', 'qwen3-vl-flash', 'B000001', '2026-08-13T00:01:00Z');"
        ).await.unwrap();
        connection.close().await.unwrap();

        let database = Database::connect(&path).await.unwrap();
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM orders WHERE id = 'order-a' AND image_sha256 = 'order-hash'",
            )
            .await,
            1
        );
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM order_import_logs WHERE task_id = 'order-a' AND provider = 'aliyun_batch' AND model = 'qwen3-vl-flash'",
            )
            .await,
            1
        );
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM pragma_table_info('order_import_logs') WHERE name = 'batch_short_number'",
            )
            .await,
            0
        );
    }

    #[tokio::test]
    async fn resumes_interrupted_batch_removal_migration() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("resume-remove-batch.sqlite3");
        let connection = connect_raw(&path).await;
        Migrator::up(&connection, Some(5)).await.unwrap();
        connection.execute_unprepared(
            "INSERT INTO order_import_pool (id, original_file_name, managed_image_path, image_sha256, status, retry_count, created_at) VALUES ('pending', 'pending.png', 'images/pending.png', 'pending-hash', 'pending', 0, '2026-08-13T00:00:00Z');
             INSERT INTO order_import_pool (id, original_file_name, managed_image_path, image_sha256, status, retry_count, created_at) VALUES ('locked', 'locked.png', 'images/locked.png', 'locked-hash', 'batch_locked', 1, '2026-08-13T00:01:00Z');
             ALTER TABLE order_import_pool RENAME TO order_import_pool_m0006_old;
             CREATE TABLE order_import_pool (id TEXT PRIMARY KEY, original_file_name TEXT NOT NULL, managed_image_path TEXT NOT NULL, image_sha256 TEXT NOT NULL, status TEXT NOT NULL CHECK (status IN ('pending', 'processing', 'failed')), error_code TEXT, error_message TEXT, retry_count INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL, started_at TEXT);
             INSERT INTO order_import_pool (id, original_file_name, managed_image_path, image_sha256, status, retry_count, created_at) VALUES ('pending', 'pending.png', 'images/pending.png', 'pending-hash', 'pending', 0, '2026-08-13T00:00:00Z');"
        ).await.unwrap();
        connection.close().await.unwrap();

        let database = Database::connect(&path).await.unwrap();
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM order_import_pool",
            )
            .await,
            2
        );
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM order_import_pool WHERE id = 'locked' AND status = 'failed' AND error_code = 'batch_api_removed'",
            )
            .await,
            1
        );
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM sqlite_master WHERE type = 'table' AND name LIKE '%m0006_old'",
            )
            .await,
            0
        );
    }

    #[tokio::test]
    async fn migrated_invoice_constraints_are_enforced() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("constraints.sqlite3");
        let database = Database::connect(&path).await.unwrap();
        database.connection.execute_unprepared(
            "INSERT INTO invoice_batches (id, status, created_at) VALUES ('batch', 'processing', '2026-08-12T00:00:00Z');
             INSERT INTO invoice_tasks (id, batch_id, original_file_name, source_sha256, media_type, status, retry_count, created_at) VALUES ('task', 'batch', 'invoice.pdf', 'hash', 'application/pdf', 'processing', 0, '2026-08-12T00:00:00Z');
             INSERT INTO invoices (id, task_id, type, subtype, type_description, subtype_description, reliable_identity, currency, preview_path, raw_invoice_json, created_at, updated_at) VALUES ('invoice-a', 'task', 3, 'VatSpecialInvoice', '增值税发票', '增值税专用发票', 'vat|4403|1001', 'CNY', 'invoices/previews/a.pdf', '{}', '2026-08-12T00:00:00Z', '2026-08-12T00:00:00Z');
             INSERT INTO invoice_items (invoice_id, position, name, raw_json) VALUES ('invoice-a', 0, '服务', '{}');
             INSERT INTO invoice_source_links (invoice_id, task_id, page_start, page_end) VALUES ('invoice-a', 'task', 1, 1);"
        ).await.unwrap();

        let duplicate = database.connection.execute_unprepared(
            "INSERT INTO invoices (id, task_id, type, subtype, type_description, subtype_description, reliable_identity, currency, preview_path, raw_invoice_json, created_at, updated_at) VALUES ('invoice-b', 'task', 3, 'VatSpecialInvoice', '增值税发票', '增值税专用发票', 'vat|4403|1001', 'CNY', 'invoices/previews/b.pdf', '{}', '2026-08-12T00:00:00Z', '2026-08-12T00:00:00Z')"
        ).await;
        assert!(duplicate.is_err());

        database
            .connection
            .execute_unprepared("DELETE FROM invoice_tasks WHERE id = 'task'")
            .await
            .unwrap();
        for table in ["invoices", "invoice_items", "invoice_source_links"] {
            assert_eq!(
                scalar(
                    &database.connection,
                    &format!("SELECT COUNT(*) count FROM {table}"),
                )
                .await,
                0,
                "{table} should be removed through foreign-key cascade"
            );
        }
    }

    #[tokio::test]
    async fn upgrades_m0003_with_existing_orders_and_invoices() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("upgrade-matching.sqlite3");
        let connection = connect_raw(&path).await;
        Migrator::up(&connection, Some(3)).await.unwrap();
        connection.execute_unprepared(
            "PRAGMA foreign_keys = ON;
             INSERT INTO import_batches (id, platform, status, created_at) VALUES ('order-batch', 'taobao', 'completed', '2026-08-12T00:00:00Z');
             INSERT INTO import_tasks (id, batch_id, original_file_name, image_sha256, status, retry_count, created_at) VALUES ('order-a', 'order-batch', 'order.png', 'order-hash', 'succeeded', 0, '2026-08-12T00:00:00Z');
             INSERT INTO orders (id, platform, external_order_number, paid_amount_fen, raw_ocr_json, created_at, updated_at) VALUES ('order-a', 'taobao', 'ORDER-A', 1000, '{}', '2026-08-12T00:00:00Z', '2026-08-12T00:00:00Z');
             INSERT INTO invoice_batches (id, status, created_at) VALUES ('invoice-batch', 'completed', '2026-08-12T00:00:00Z');
             INSERT INTO invoice_tasks (id, batch_id, original_file_name, source_sha256, media_type, status, retry_count, created_at) VALUES ('invoice-task', 'invoice-batch', 'invoice.pdf', 'invoice-hash', 'application/pdf', 'succeeded', 0, '2026-08-12T00:00:00Z');
             INSERT INTO invoices (id, task_id, type, subtype, type_description, subtype_description, total_fen, currency, preview_path, raw_invoice_json, created_at, updated_at) VALUES ('invoice-a', 'invoice-task', 3, 'VatInvoice', '增值税发票', '增值税发票', 1000, 'CNY', 'invoices/previews/a.pdf', '{}', '2026-08-12T00:00:00Z', '2026-08-12T00:00:00Z');"
        ).await.unwrap();
        connection.close().await.unwrap();

        let database = Database::connect(&path).await.unwrap();
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM orders WHERE id='order-a'"
            )
            .await,
            1
        );
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM invoices WHERE id='invoice-a'"
            )
            .await,
            1
        );
        assert_eq!(scalar(&database.connection, "SELECT COUNT(*) count FROM seaql_migrations WHERE version='m0004_order_invoice_matching'").await, 1);
    }

    #[tokio::test]
    async fn resumes_interrupted_matching_migration() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("resume-matching.sqlite3");
        let connection = connect_raw(&path).await;
        Migrator::up(&connection, Some(3)).await.unwrap();
        connection.execute_unprepared(
            "CREATE TABLE invoice_matches (invoice_id TEXT PRIMARY KEY REFERENCES invoices(id) ON DELETE CASCADE, source TEXT NOT NULL CHECK (source IN ('automatic', 'custom')), confirmed_at TEXT NOT NULL)"
        ).await.unwrap();
        connection.close().await.unwrap();

        let database = Database::connect(&path).await.unwrap();
        assert_eq!(scalar(&database.connection, "SELECT COUNT(*) count FROM sqlite_master WHERE type='table' AND name IN ('invoice_matches','invoice_match_orders')").await, 2);
        assert_eq!(scalar(&database.connection, "SELECT COUNT(*) count FROM seaql_migrations WHERE version='m0004_order_invoice_matching'").await, 1);
    }

    #[tokio::test]
    async fn matching_constraints_prevent_order_reuse_and_cascade() {
        let database = Database::in_memory().await.unwrap();
        database.connection.execute_unprepared(
            "INSERT INTO import_batches (id, platform, status, created_at) VALUES ('ob', 'taobao', 'completed', '2026-08-12T00:00:00Z');
             INSERT INTO import_tasks (id, batch_id, original_file_name, image_sha256, status, retry_count, created_at) VALUES ('order-a', 'ob', 'order.png', 'oh', 'succeeded', 0, '2026-08-12T00:00:00Z');
             INSERT INTO orders (id, platform, paid_amount_fen, raw_ocr_json, created_at, updated_at) VALUES ('order-a', 'taobao', 1000, '{}', '2026-08-12T00:00:00Z', '2026-08-12T00:00:00Z');
             INSERT INTO invoice_batches (id, status, created_at) VALUES ('ib', 'completed', '2026-08-12T00:00:00Z');
             INSERT INTO invoice_tasks (id, batch_id, original_file_name, source_sha256, media_type, status, retry_count, created_at) VALUES ('it', 'ib', 'invoice.pdf', 'ih', 'application/pdf', 'succeeded', 0, '2026-08-12T00:00:00Z');
             INSERT INTO invoices (id, task_id, type, subtype, type_description, subtype_description, currency, preview_path, raw_invoice_json, created_at, updated_at) VALUES ('invoice-a', 'it', 3, 'VatInvoice', '发票', '发票', 'CNY', 'a.pdf', '{}', '2026-08-12T00:00:00Z', '2026-08-12T00:00:00Z'), ('invoice-b', 'it', 3, 'VatInvoice', '发票', '发票', 'CNY', 'b.pdf', '{}', '2026-08-12T00:00:00Z', '2026-08-12T00:00:00Z');
             INSERT INTO invoice_matches (invoice_id, source, confirmed_at) VALUES ('invoice-a','automatic','2026-08-12T00:00:00Z'), ('invoice-b','automatic','2026-08-12T00:00:00Z');
             INSERT INTO invoice_match_orders (invoice_id, order_id, position) VALUES ('invoice-a','order-a',0);"
        ).await.unwrap();
        assert!(database.connection.execute_unprepared("INSERT INTO invoice_match_orders (invoice_id, order_id, position) VALUES ('invoice-b','order-a',0)").await.is_err());
        database
            .connection
            .execute_unprepared("DELETE FROM invoice_matches WHERE invoice_id='invoice-a'")
            .await
            .unwrap();
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM invoice_match_orders"
            )
            .await,
            0
        );
    }
}
