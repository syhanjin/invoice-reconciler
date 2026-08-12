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
| `invoice_batches` | UUIDv7 text | invoice-specific lifecycle and optional forced Tencent type |
| `invoice_tasks` | UUIDv7 text | cascades from batch; stores source hash, audit response, and retry state |
| `invoices` | UUIDv7 text | cascades from task; reliable identity is unique when present |
| `invoice_items` | integer | `(invoice_id, position)` is unique and cascades from invoice |
| `invoice_source_links` | integer | invoice/task/page tuple is unique and cascades from both parents |
| `invoice_recovery_markers` | `(source_sha256, recovery_identity)` | records deleted subtype/number identities and optionally binds one recovery task |
| `invoice_matches` | invoice UUIDv7 text | one confirmed relationship per invoice; source is `automatic` or `custom` |
| `invoice_match_orders` | `(invoice_id, order_id)` | order ID is globally unique and positions are unique within one invoice |

Money is stored as integer fen. Date/time values are normalized strings in `YYYY-MM-DD HH:mm` form. Booleans use SQLite integers through SeaORM models. The task, order, and managed image basename share the same UUID.

## Query And Transaction Patterns

- Use typed SeaORM entities for ordinary create, read, update, and delete work.
- Raw SQL through `Statement` is allowed for aggregate/search queries that SeaORM cannot express clearly, but bind every user-provided value; never interpolate it into SQL.
- Server-paged sort queries must whitelist SQL column and direction fragments from typed enums and append a deterministic unique-key tie-breaker; never interpolate client strings or rely on non-unique primary ordering alone.
- Wrap order edits, product replacement, and duplicate-flag recalculation in one transaction.
- Treat `import_tasks` as durable audit history. Business deletion of an order deletes the `orders` row and cascading items but preserves its task; clear only `managed_image_path` in the same transaction to release the source hash.
- Write invoice recovery markers in the same transaction as invoice deletion. Claim all unclaimed markers for one source hash in the same transaction that creates its recovery task; consume matched markers and release unmatched markers in the persistence transaction.
- External order numbers may repeat. After inserting, editing, or deleting an order, recalculate `possible_duplicate` for every affected non-empty order-number group.
- Treat SQLite as the source of truth. In-memory queue and Jotai state are projections, not durable state.
- Keep matching candidates ephemeral. Persist only confirmed `invoice_matches` and members; use one transaction for batch confirmation and rely on the unique order index as the final occupancy guard.

## Migrations

Add a new `MigrationTrait` implementation and append it to `Migrator::migrations()`. Never edit an applied migration merely to evolve an existing installation. Every new constraint or index needs a migration test that creates a fresh database and asserts both the schema behavior and the repository behavior it supports.

- Run `Migrator::up` before any startup query references a table introduced by a pending migration. Restart recovery updates for `queued` or `processing` rows run only after all pending migrations finish.
- SQLite migration DDL is not wrapped in one transaction by `sea-orm-migration`. New table and index statements must therefore be safely repeatable with `IF NOT EXISTS`, so a process interruption can resume instead of failing on a partially created schema.
- Migration tests must use real temporary SQLite files for both a fresh install and an upgrade from the immediately previous migration. Seed existing domain data before upgrading and assert it survives.
- Also simulate a partial prior migration, reconnect through the production `Database::connect` path, and assert the missing schema plus migration ledger entry are completed.
- Verify constraints behavior after migration: foreign-key cascades, partial unique indexes, check constraints, and required query indexes must be exercised through actual writes or schema inspection.

## File Compensation

Database transactions cannot include filesystem changes. Use explicit compensation:

- Import: copy the UUID-named image, then insert task data; on any creation failure, delete the batch and copied files.
- Order delete: move the managed image to a temporary trash path, transactionally delete the order and clear the preserved task's managed path, then remove the trash file. Restore the image if either database mutation fails.
- Bulk delete: apply the same compensated single-order operation sequentially per unique ID. Return per-ID outcomes so one failure neither hides prior successes nor prevents later IDs from being attempted.
- Resolve all managed paths under the configured application-data directory before reading, deleting, or exposing them.

## Forbidden Patterns

- Do not use an external order number as a primary key or unique key.
- Do not store decimal currency in floating-point columns.
- Do not delete the managed image before arranging a recoverable compensation path.
- Do not delete an import task as a side effect of deleting its business order.
- Do not modify duplicate flags outside the same transaction as the associated order change.
- Do not use ad hoc schema creation outside `sea-orm-migration`.
- Do not run restart-recovery SQL for new tables before `Migrator::up`.
- Do not assume SQLite migration DDL is automatically atomic.
- Do not persist matching candidates or confirm a multi-item batch with one transaction per item.

## Required Tests

- Fresh file migration, previous-version upgrade with seeded data, and interrupted-migration recovery.
- Foreign-key cascade and reliable-identity unique-index enforcement after invoice migration.
- Duplicate order-number group creation, edit between groups, and delete recalculation.
- Unique SHA-256 behavior for managed images while skipped tasks remain recordable.
- Invoice source-hash behavior for source-less audit tasks, partial-delete recovery, no-match release, and hash re-blocking after recovery completes.
- Transaction rollback for failed order/item mutations.
- Bulk-delete partial-result coverage, preserved import-task/batch history, duplicate-group recalculation after successful items, and a fault-injection test proving the order, task path, hash claim, and managed image are restored when database deletion fails.
- Startup conversion of `queued` and `processing` tasks to `interrupted`.
- Matching fresh-file/m0003-upgrade/interrupted-migration coverage, unique order occupancy, cascade cleanup, and atomic batch rollback.
