# Database Guidelines

> SQLite persistence is owned by the Rust process and implemented with SeaORM.

## Stack And Ownership

- Use `sea-orm` for application queries and entity mutations.
- Use `sea-orm-migration` for every schema change. Migrations run from `Migrator::up` during Tauri startup.
- The React application must never open SQLite or construct SQL.
- Keep entity definitions in `src-tauri/src/entities.rs`, migrations in `migration.rs`, and repository-style operations on `Database` in `db.rs`.
- SQLite foreign keys are enabled when the connection is created.

Do not add a second application-facing database library. SeaORM may use SQLx internally, but feature code must use SeaORM APIs and the shared `DatabaseConnection`.

## Schema Contracts

| Table | Primary key | Important constraints |
| --- | --- | --- |
| `app_settings` | fixed integer `1` | concurrency is `1..=8` |
| `import_batches` | UUIDv7 text | platform and lifecycle status are checked |
| `import_tasks` | UUIDv7 text | cascades from batch; managed image hashes are unique |
| `orders` | task UUIDv7 text | cascades from task; external order number is not unique |
| `order_items` | integer | `(order_id, position)` is unique and cascades from order |

Money is stored as integer fen. Date/time values are normalized strings in `YYYY-MM-DD HH:mm` form. Booleans use SQLite integers through SeaORM models. The task, order, and managed image basename share the same UUID.

## Query And Transaction Patterns

- Use typed SeaORM entities for ordinary create, read, update, and delete work.
- Raw SQL through `Statement` is allowed for aggregate/search queries that SeaORM cannot express clearly, but bind every user-provided value; never interpolate it into SQL.
- Server-paged sort queries must whitelist SQL column and direction fragments from typed enums and append a deterministic unique-key tie-breaker; never interpolate client strings or rely on non-unique primary ordering alone.
- Wrap order edits, product replacement, and duplicate-flag recalculation in one transaction.
- External order numbers may repeat. After inserting, editing, or deleting an order, recalculate `possible_duplicate` for every affected non-empty order-number group.
- Treat SQLite as the source of truth. In-memory queue and Jotai state are projections, not durable state.

## Migrations

Add a new `MigrationTrait` implementation and append it to `Migrator::migrations()`. Never edit an applied migration merely to evolve an existing installation. Every new constraint or index needs a migration test that creates a fresh in-memory database and asserts both the schema behavior and the repository behavior it supports.

## File Compensation

Database transactions cannot include filesystem changes. Use explicit compensation:

- Import: copy the UUID-named image, then insert task data; on any creation failure, delete the batch and copied files.
- Delete: move the managed image to a temporary trash path, delete database rows transactionally, then remove the trash file. Restore the image if the database operation fails.
- Resolve all managed paths under the configured application-data directory before reading, deleting, or exposing them.

## Forbidden Patterns

- Do not use an external order number as a primary key or unique key.
- Do not store decimal currency in floating-point columns.
- Do not delete the managed image before arranging a recoverable compensation path.
- Do not modify duplicate flags outside the same transaction as the associated order change.
- Do not use ad hoc schema creation outside `sea-orm-migration`.

## Required Tests

- Fresh migration and migration idempotency.
- Duplicate order-number group creation, edit between groups, and delete recalculation.
- Unique SHA-256 behavior for managed images while skipped tasks remain recordable.
- Transaction rollback for failed order/item mutations.
- Startup conversion of `queued` and `processing` tasks to `interrupted`.
