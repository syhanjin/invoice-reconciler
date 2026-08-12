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
                "SELECT COUNT(*) count FROM seaql_migrations WHERE version IN ('m0001_initial', 'm0002_invoices', 'm0003_invoice_recovery')",
            )
            .await,
            3
        );
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM sqlite_master WHERE type = 'table' AND name IN ('invoice_batches', 'invoice_tasks', 'invoices', 'invoice_items', 'invoice_source_links', 'invoice_recovery_markers')",
            )
            .await,
            6
        );
        assert_eq!(
            scalar(
                &database.connection,
                "SELECT COUNT(*) count FROM sqlite_master WHERE type = 'index' AND name = 'idx_invoices_reliable_identity'",
            )
            .await,
            1
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
}
