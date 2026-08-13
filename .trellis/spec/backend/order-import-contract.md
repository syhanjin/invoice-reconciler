# Order Import Contract

## Scenario: Unified Realtime Order Recognition Pool

### 1. Scope / Trigger

This contract applies whenever a Tauri command, SQLite migration, managed-image operation, recognition provider, queue transition, settings payload, or frontend projection involved in order screenshot import changes. Orders use one durable import pool; do not reintroduce a user-visible order batch. Invoice batches and the Tencent invoice queue are a separate domain.

### 2. Signatures

Tauri commands use camel-case serialized request and response fields:

```text
bootstrap() -> BootstrapState
get_settings() -> AppSettings
save_settings(request: UpdateSettingsRequest) -> AppSettings
check_llm(request: UpdateSettingsRequest) -> LlmStatus
import_order_images(request: { platform: "taobao", imagePaths: string[] }) -> ImportOrderImagesResult
get_order_import_pool() -> OrderImportPool
list_order_import_logs(query: OrderImportLogsQuery) -> OrderImportLogsPage
retry_order_import_task(request: { id }) -> bool
retry_failed_order_import_tasks() -> u64
delete_order_import_task(request: { id }) -> ()
```

The backend emits `order-import-updated` as `{ kind, taskId?, orderId? }` after durable changes. It is an invalidation signal; consumers reload authoritative pool, log, and order projections.

Durable order-import tables are `order_import_pool`, `order_import_logs`, `orders`, and `order_items`. Legacy `import_batches` and `import_tasks` remain for schema compatibility only.

### 3. Contracts

#### Global LLM settings

`llmProvider` is `ollama` or `openai`. Provider, endpoint, model, and optional API key form one global LLM configuration shared by order image recognition and confirmed-match product summaries. Order recognition separately owns `orderRecognitionMaxConcurrency` in `1..=8`; summary generation is always single concurrency.

- Ollama uses `/api/chat`, has no key, and preserves its own base URL/model.
- OpenAI-compatible recognition uses `/chat/completions`, an optional bearer key, and its own base URL/model.

Runtime settings live in versioned application-data `settings.toml`. Version 4 stores provider endpoints under `[llm]`, keeps only concurrency under `[order_recognition]`, and preserves the existing Tencent/matching sections. Public `AppSettings` exposes only masked/configured credential state. A blank key preserves the stored value and an explicit clear flag removes it. Version 1/2/3 files upgrade directly to version 4; version 2 `provider = "aliyun_batch"` falls back to Ollama and erases the retired Aliyun key, while version 3 preserves the selected provider, both endpoints/models, OpenAI key, and concurrency. Replacement remains atomic with `0600` on Unix or a current-user ACL on Windows.

#### Pool and logs

`OrderImportPool` contains `{ tasks, pending, processing, failed }`. Task states are:

```text
pending -> processing -> removed on success
processing -> failed on request/persistence failure or startup interruption
failed -> pending on explicit retry
```

Only `pending` and `failed` tasks may be deleted. A successful recognition transaction creates the order/items, transfers image ownership to `orders`, inserts one immutable log, and removes the pool row. Completion requires the task to be `processing`; replay after success is idempotent by task ID.

Logs store provider/model, warning, filename, order ID, and completion time. Historical rows may contain `provider = "aliyun_batch"`; preserve that text as audit data. Logs have no batch number, never own images, and are not changed when an order is edited or deleted.

#### Realtime recognition

The process-wide queue consumes every pending task. New imports and explicit retries wake it. Each drain re-reads current private settings and respects the current concurrency ceiling. A provider change applies to newly claimed work; already-sent requests may finish.

Ollama sends `stream: false`, JSON Schema in `format`, `temperature: 0`, and `num_ctx: 16384`. OpenAI-compatible recognition sends a base64 data URL, strict `response_format.type = json_schema`, and bearer auth when configured. Both normalize the same order fields, store money as integer fen, normalize valid local time to `YYYY-MM-DD HH:mm`, and preserve provider JSON in `raw_ocr_json`.

#### Batch-removal migration

Migration `m0006_remove_aliyun_batch` runs before startup recovery. It rebuilds the pool/log tables without batch fields, drops `order_recognition_batches`, and converts legacy `batch_submitting`/`batch_locked` tasks to `failed` with code `batch_api_removed` and an actionable manual-retry message. It must not contact Aliyun, auto-retry the task, or delete its managed image.

The migration uses resumable `_m0006_old` backup tables. On rerun, copy missing rows with `INSERT OR IGNORE`, convert legacy statuses, then remove the backup. Preserve completed orders, matches, and provider/model audit text.

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Empty image list or more than 500 selected images | Return `invalid_input`; copy and persist nothing |
| Unsupported extension, invalid magic bytes, or unreadable file | Reject before pool persistence |
| Duplicate SHA-256 in request, pool, or image-owning order | Count as skipped; create no task/log and make no OCR call |
| Realtime request or JSON failure | Mark only that task failed; require explicit retry |
| Process exits during recognition | Mark `processing` failed at startup; do not auto-retry |
| Delete a processing task | Return `invalid_input`; preserve row and image |
| Pool deletion database failure | Restore its temporarily moved image |
| Version 2 settings select Aliyun | Migrate provider to Ollama and erase Aliyun key from the version 4 file |
| Version 3 settings contain an OpenAI key | Move the endpoint/model/key under `[llm.openai]` without exposing or losing the key |
| Legacy task is batch locked/submitting | Migrate to failed/manual retry; preserve image and hash |
| Historical log provider is Aliyun | Return provider/model unchanged as read-only audit data |
| Delete a recognized order | Remove its owned image/hash claim; preserve immutable log |

### 5. Good/Base/Bad Cases

- Good: new screenshots join the durable pool and use the same current model configuration that later generates confirmed-match summaries.
- Good: an m0005 database with locked Aliyun work upgrades with those images retained as failed, manually retryable tasks.
- Good: a historical Aliyun order remains matched and its log still identifies its original provider/model after upgrade.
- Base: a response omits an optional date; valid fields produce an order and immutable log with a warning.
- Bad: reintroducing submit/cancel Batch commands, exposing an Aliyun settings option, auto-retrying migrated work, deleting historical results, or mutating logs violates this contract.

### 6. Tests Required

- Migration: fresh file, m0005 upgrade with normal/locked/completed data, and interrupted m0006 recovery from backup tables.
- Settings: version 1 upgrade, version 2 Aliyun fallback/key removal, lossless version 3 to 4 promotion, atomic writes, credential masking, and permission hardening.
- Pool database: atomic/idempotent completion, retry transitions, task deletion compensation, and immutable logs.
- Realtime queue: peak in-flight work never exceeds configured concurrency.
- Protocol mocks: Ollama/OpenAI paths, bearer auth, image data URL, strict schema, and normalized response.
- Frontend: two-provider settings, pool states, retry/delete rules, historical Aliyun source label, pagination, and event-driven reload.
- Quality gate: strict Clippy, full Rust tests, `bun run lint`, `bun run test`, and `bun run build`.

### 7. Wrong vs Correct

#### Wrong

```rust
// Migrated remote work must not silently run under a different provider.
UPDATE order_import_pool SET status = 'pending' WHERE status = 'batch_locked';
```

#### Correct

```rust
// Preserve user control and make the migration reason actionable.
UPDATE order_import_pool
SET status = 'failed', error_code = 'batch_api_removed'
WHERE status IN ('batch_submitting', 'batch_locked');
```

#### Wrong

```ts
type LlmProvider = "ollama" | "openai" | "aliyun_batch"
```

#### Correct

```ts
type LlmProvider = "ollama" | "openai"
```

Aliyun remains valid only as a historical string in immutable log data and legacy migration inputs, never as a current provider or executable code path.
