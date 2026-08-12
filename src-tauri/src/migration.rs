use sea_orm::{ConnectionTrait, Statement};
use sea_orm_migration::prelude::*;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(M0001Initial)]
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
