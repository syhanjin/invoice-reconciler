# Invoice Import Contract

## Scenario: Tencent OCR Import, Review, And Ledger

### 1. Scope / Trigger

This contract applies when changing `settings.toml`, Tencent Cloud OCR signing or requests, invoice preflight, the invoice queue, response normalization, preview splitting, invoice persistence, deduplication, review grouping, or invoice Tauri commands. Invoice OCR is a paid external operation, so preflight, retry, and restart behavior are part of the durable contract.

### 2. Signatures

Public Tauri commands use one camel-case `request` or `query` object:

```text
get_settings() -> AppSettings
save_settings(request: UpdateSettingsRequest) -> AppSettings

prepare_invoice_batch(request: { filePaths: string[], forcedType: number | null }) -> InvoiceBatchPreview
create_invoice_batch(request: { preflightToken: string }) -> InvoiceBatchDetail
list_invoice_batches(query: { page?: number, pageSize?: number }) -> InvoiceBatchesPage
get_invoice_batch(request: { id: string }) -> InvoiceBatchDetail
cancel_invoice_task(request: { id: string }) -> boolean
retry_invoice_task(request: { id: string, forcedType: number | null }) -> InvoiceBatchDetail
confirm_invoice_split(request: { taskId: string, groups: number[][] }) -> InvoiceBatchDetail

list_invoices(query: { ..., projectId?: string, unassignedOnly?: boolean }) -> InvoicesPage
get_invoice(request: { id: string }) -> InvoiceDetail
update_invoice(request: UpdateInvoiceRequest) -> InvoiceDetail
delete_invoice(request: { id: string }) -> void
delete_invoices(request: { ids: string[] }) -> DeleteInvoicesResult
```

Durable schema:

```text
invoice_batches 1 -- * invoice_tasks 1 -- * invoices 1 -- * invoice_items
                                      `-- * invoice_source_links * -- 1 invoices
```

`settings.toml` is version `1` and has `[ollama]` plus `[tencent_cloud]`. SQLite retains the old Ollama settings row only as the one-time migration source.

### 3. Contracts

- `settings.toml` lives in the Tauri application-data directory. Writes use a private same-directory temporary file, flush it, and atomically replace the destination. Unix mode is `0600`; Windows restricts the ACL to the current user and uses replace-existing/write-through semantics.
- Permission tightening failure does not stop startup or save. It becomes `permissionWarning` and remains visible in Settings.
- `get_settings` returns only a masked SecretId and a configured boolean. SecretKey is never serialized into a public response, event, log, SQLite row, or test snapshot.
- If no TOML exists, migrate Ollama URL, model, and concurrency from SQLite. After that, the TOML store is authoritative for Ollama and Tencent settings.
- Preflight is local-only, accepts at most `500` PNG/JPG/JPEG/PDF files, validates magic bytes, image dimensions, PDF page count, and Base64 encoded size, then returns a single-use token valid for ten minutes.
- Create consumes the token, rechecks every file fingerprint, then copies valid sources and writes tasks. It checks the durable SHA-256 set again during creation to close the preflight race. Duplicate and invalid selections become failed audit tasks and make zero OCR calls.
- Tencent calls `RecognizeGeneralInvoice` version `2018-11-19` at `ocr.tencentcloudapi.com`, with `EnableOther`, `EnablePdf`, `EnableMultiplePage`, and `EnableCutImage`. `Types` is omitted for automatic recognition and contains exactly one validated type when forced.
- TC3 uses UTC date, `ocr` service scope, the exact JSON body hash, and the exact sent content-type and host headers. A shared limiter leaves at least `200 ms` between attempts. Invoice concurrency is independently clamped to `1..=5`.
- Only transient network, HTTP 429/5xx, and Tencent limit/internal errors are retried, at most three total attempts with bounded exponential delay.
- On startup, pending migrations run first. Then queued/processing invoice tasks become `interrupted`; they are never automatically retried.
- Adjacent pages with the same reliable identity auto-merge. Non-adjacent repeated identities, adjacent same-subtype pages without identity, or same-page results without an independent cut preview enter `awaiting_review`.
- Review groups cover every recognized item exactly once. Same group merges without another OCR call. Splitting one reliable identity into multiple groups is rejected; merging inconsistent identities clears the reliable identity and marks the result as a possible duplicate.
- Preview priority is Tencent `CutImage`, then page-range PDF extraction, then image polygon crop, then source copy. Each final ledger row owns a preview; all rows from one task may share the immutable source.
- If one mixed response contains both successful and failed invoice items, persist the successful records and keep every failed item code in `warning_message`; never silently treat the source as fully recognized.
- Reliable identity collision is a hard duplicate: no invoice row is committed, the task keeps RequestId/raw response/existing ID, previews are removed, and the duplicate source copy is deleted when possible. Weak number collisions remain in the ledger with `possibleDuplicate` on the full affected group.
- Invoice edit recomputes reliable identity and both old/new weak duplicate groups transactionally. Delete uses file trash/restore compensation and removes the source only when deleting the final invoice for the task.
- Invoice ledger rows include nullable project identity/name and support server-side project/unassigned filters. Deleting a project member relies on the membership cascade, then compacts and touches that project inside the invoice deletion transaction.
- Invoice delete stores a recovery marker keyed by source SHA-256 plus normalized subtype/invoice number in the same transaction. Any unclaimed marker lets that source pass preflight and atomically binds to one recovery task. Recovery OCR persists only marked missing identities, skips ledger rows that still exist, ignores unrelated OCR results, and consumes only matched markers. Unmatched markers are released for another attempt. Once every marker is consumed, active managed sources claim the hash again. Tasks whose managed source has been cleared are audit history and never claim the hash.
- `invoice-task-updated` is separate from the order event and carries the full task and batch detail.

### 4. Validation & Error Matrix

| Condition | Result |
| --- | --- |
| Missing Tencent credentials with valid files | `invalid_input`; no batch and no OCR call |
| Empty selection or more than 500 files | `invalid_input`; no preflight token |
| Unsupported extension, magic mismatch, invalid dimensions, PDF outside 1..=30, or Base64 over 10 MiB | Per-file `invalid_file`; valid siblings continue |
| Hash already exists or repeats in selection/create race | Per-file `duplicate_file`; zero OCR calls for that file |
| Hash has an unclaimed deletion recovery marker | Allow confirmed OCR and restore only matching missing invoices |
| Expired/consumed token or changed fingerprint | `invalid_input`; require a new preflight |
| Forced type outside the documented allowlist | `invalid_input` |
| Tencent permanent business error | Task `failed` with safe error text |
| Restart during queued/processing | Task `interrupted`, manual retry only |
| Failed/interrupted task has no managed source | Retry rejected with `invalid_input` |
| Reliable identity already exists | Task `duplicate_invoice`; no new ledger row |
| Review group is empty, duplicated, incomplete, or out of range | `invalid_input`; no persistence and no OCR call |
| Bulk delete has 0 or more than 100 IDs | `invalid_input` |
| One bulk delete item fails | Return its failure and continue remaining unique IDs |

### 5. Good/Base/Bad Cases

- Good: preflight reports two valid PDFs and one duplicate; confirmation creates three tasks, calls Tencent twice, and preserves the duplicate audit without copying or charging it.
- Good: a two-page invoice with the same code and number becomes one ledger row with a two-page PDF preview; two unrelated pages become two rows.
- Good: an ambiguous response pauses in review and the confirmed grouping persists from stored raw JSON without invoking Tencent again.
- Base: an invoice with no reliable identity is retained; a later same-subtype/same-number row marks both as possible duplicates.
- Bad: invoking Tencent before user confirmation, retrying interrupted work automatically, treating invoice number alone as globally reliable for every subtype, or returning SecretKey in `AppSettings`.

### 6. Tests Required

- Fixed-vector TC3 authorization assertion and direct `>=190 ms` limiter assertion.
- Mixed response normalization including integer fen, dates, common parties, line items, adjacent identity merge, and ambiguous grouping.
- Local file validation for extension/magic, encoded-size, image dimensions, PDF count, and managed path containment.
- Settings migration, atomic save, Unix `0600`, masked public settings, and serialized response exclusion of SecretKey.
- Invoice concurrency bound; duplicate file zero-call behavior; reliable hard duplicate; weak duplicate group insert/edit/delete recalculation; missing-source retry rejection.
- Delete/recovery tests: single- and multi-invoice sources, partial restoration, unrelated OCR results ignored, source-less audits ignored by hash deduplication, no-match marker release, delete rollback, and hash re-blocking after all markers are consumed.
- Fresh on-disk migration, `m0001` upgrade with seeded orders, partial `m0002` recovery, migration-ledger entries, foreign-key cascades, and reliable-identity uniqueness.
- Preview extraction/crop and delete compensation failure tests when those paths change.
- Frontend preflight-before-create, review grouping, subtype filter, edit/delete, multi-select bulk-delete, task-event, and PDF preview tests.
- Run `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-features`, `bun run lint`, `bun run test`, and `bun run build`.

### 7. Wrong vs Correct

#### Wrong

```rust
connection.execute("UPDATE invoice_tasks ...").await?;
Migrator::up(&connection, None).await?;
```

This fails on fresh and pre-invoice databases because `invoice_tasks` does not exist yet.

#### Correct

```rust
Migrator::up(&connection, None).await?;
connection.execute("UPDATE invoice_tasks ...").await?;
```

Run schema installation before startup recovery, make SQLite DDL repeatable, and test the production connection path against both fresh and seeded prior-version files.
