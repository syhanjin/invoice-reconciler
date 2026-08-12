# Order Import Contract

## Scenario: Taobao Screenshot OCR Import

### 1. Scope / Trigger

This contract applies whenever a Tauri command, database schema, managed-image operation, Ollama request, queue transition, or frontend payload involved in screenshot import changes. The first supported platform is `taobao`; one image produces at most one order and an order may contain multiple product items.

### 2. Signatures

Tauri commands use camel-case serialized request and response fields:

```text
bootstrap() -> BootstrapState
get_settings() -> AppSettings
save_settings(request: UpdateSettingsRequest) -> AppSettings
check_ollama(request: UpdateSettingsRequest) -> OllamaStatus
create_import_batch(request: CreateBatchRequest) -> BatchDetail
get_batch(request: { id }) -> BatchDetail
list_import_batches(query: ImportBatchesQuery) -> ImportBatchesPage
cancel_task(request: { id }) -> bool
cancel_batch(request: { id }) -> u64
retry_task(request: { id }) -> BatchDetail
retry_failed_batch(request: { id }) -> BatchDetail
list_orders(query: OrdersQuery) -> OrdersPage
get_order(request: { id }) -> OrderDetail
update_order(request: UpdateOrderRequest) -> OrderDetail
delete_order(request: { id }) -> ()
```

The backend emits `import-task-updated` with `{ task: ImportTask, batch: BatchDetail }` after durable state changes.

Durable tables are `app_settings`, `import_batches`, `import_tasks`, `orders`, and `order_items`. See [Database Guidelines](./database-guidelines.md) for keys and transaction rules.

### 3. Contracts

`UpdateSettingsRequest` contains `ollamaBaseUrl: string`, `ollamaModel: string`, and `maxConcurrency: number`. Defaults are `http://127.0.0.1:11434`, `qwen3-vl:4b`, and `3`.

`CreateBatchRequest` contains `platform: "taobao"` and `imagePaths: string[]`. A non-duplicate file receives a UUIDv7 before copying and is stored as `images/<uuid>.<normalized-extension>`. The task ID, order ID, and image basename must remain identical across retry.

`ImportBatchesQuery` contains optional `page` and `pageSize` integers. Page defaults to `1` and is normalized to at least `1`; page size defaults to `20` and is clamped to `1..=100`. `ImportBatchesPage` contains `{ items, total, page, pageSize }`. Each item is an `ImportBatchSummary` with batch metadata plus aggregate `BatchCounts`, but no `tasks` array. Sort summaries by `created_at DESC, id DESC`, then load the selected batch through `get_batch`; this keeps IPC and memory bounded as history grows.

The Ollama `/api/chat` request uses `stream: false`, `format` containing the JSON Schema, `options.temperature: 0`, and `options.num_ctx: 16384`. Its recognized JSON shape is:

```json
{
  "order_number": "string|null",
  "product_names": ["string"],
  "paid_amount": "number|null",
  "ordered_at": "string|null",
  "shop_name": "string|null"
}
```

Store valid money as integer fen and valid time as local `YYYY-MM-DD HH:mm`. Preserve the original model JSON exactly in `raw_ocr_json`. Invalid optional values become `null` and add a warning; they do not discard otherwise valid fields.

Queue concurrency is captured from settings when a run begins. The number of sent but unfinished Ollama HTTP requests must never exceed that value. Only one batch or retry run may actively own the application queue; Ollama may queue the bounded set of already-sent requests internally.

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Empty image list | Return `invalid_input`; create no batch |
| More than 500 images | Return `invalid_input`; create no batch |
| Unsupported extension, invalid magic bytes, or unreadable file | Reject before batch persistence |
| Ollama unreachable | Return actionable connection error; do not install software |
| Configured model absent | Return `model_missing` with `ollama pull <model>` guidance |
| Same SHA-256 in this or an earlier batch | Create a `skipped` task; do not copy or call OCR |
| Same non-empty order number from different images | Retain all orders and flag the complete group as possible duplicates |
| Invalid model JSON or request failure | Mark task `failed` with a stable error code/message |
| Process exits while queued/processing | Mark task `interrupted` on next startup; never auto-resubmit |
| Cancel queued task | Mark `canceled`; worker must recheck durable status before send |
| Cancel processing task | Do not force-abort the HTTP request |
| Delete database failure | Restore the image from the temporary trash path |
| Import history page below 1 | Normalize to page `1` |
| Import history page size outside `1..=100` | Clamp it before applying `LIMIT` |
| Import history is empty | Return an empty page with `total = 0`; do not synthesize a batch |

### 5. Good/Base/Bad Cases

- Good: ten images with concurrency three result in no more than three simultaneous HTTP requests, each success is committed immediately, and task events update progress.
- Base: an OCR result with no order number and an invalid date still stores valid product/shop fields, with nullable fields and a warning.
- Bad: copying an image under its original name, sending every request at once, returning all task rows in every history summary, treating a repeated order number as a hard duplicate, or retrying under a new UUID violates this contract.

### 6. Tests Required

- Migration integration test: all tables migrate and rerunning migrations succeeds.
- Database integration test: same-number groups are flagged; editing between groups recalculates both groups.
- Import-history database test: assert stable pagination, aggregate counts for every task status, empty results, and page-size clamping.
- File unit test: extension and magic-byte validation agree, and the UUID basename is retained.
- OCR mock-server test: assert the exact structured request fields and parse a valid response.
- Queue async test: measure peak in-flight requests and assert `peak <= captured_concurrency`.
- Normalization tests: decimal rounding to fen, accepted time formats, null/warning fallback.
- Frontend contract tests: invoke argument names match Rust camel-case types and task events replace the active batch snapshot.
- Desktop smoke test: imported image renders through the restricted asset protocol and edit/delete/settings flows remain usable at the minimum window size.

### 7. Wrong vs Correct

#### Wrong

```rust
for task in tasks {
    tokio::spawn(call_ollama(task));
}
```

This has no application-wide ownership and can send an unbounded number of requests.

#### Correct

```rust
stream::iter(task_ids)
    .for_each_concurrent(Some(settings.max_concurrency.into()), |task_id| async move {
        process_task(task_id).await;
    })
    .await;
```

Reserve the global queue before this loop, release it when the run finishes, and re-read each task's durable status immediately before sending.

For import history, do not build `Vec<BatchDetail>` for a list page. Query aggregate summaries first and call `batch(id)` only for the selected row.
