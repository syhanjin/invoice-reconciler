# Order Import Contract

## Scenario: Unified Order Recognition Pool

### 1. Scope / Trigger

This contract applies whenever a Tauri command, SQLite migration, managed-image operation, recognition provider, queue transition, remote batch supervisor, or frontend payload involved in order screenshot import changes. The first supported platform is `taobao`; one image produces at most one order and an order may contain multiple product items.

Orders have one durable import pool. Do not reintroduce a user-visible order import batch. Invoice batches and the Tencent invoice queue are a separate domain and are unchanged by this contract.

### 2. Signatures

Tauri commands use camel-case serialized request and response fields:

```text
bootstrap() -> BootstrapState
get_settings() -> AppSettings
save_settings(request: UpdateSettingsRequest) -> AppSettings
check_order_recognizer(request: UpdateSettingsRequest) -> RecognizerStatus
import_order_images(request: { platform: "taobao", imagePaths: string[] }) -> ImportOrderImagesResult
get_order_import_pool() -> OrderImportPool
list_order_import_logs(query: OrderImportLogsQuery) -> OrderImportLogsPage
retry_order_import_task(request: { id }) -> bool
retry_failed_order_import_tasks() -> u64
delete_order_import_task(request: { id }) -> ()
submit_order_recognition_batch() -> string
cancel_order_recognition_batch(request: { id }) -> ()
```

The backend emits `order-import-updated` after durable changes:

```text
{ kind, taskId?, orderId?, recognitionBatchId? }
```

The event is an invalidation signal. Consumers reload the authoritative pool, completion log, and order list instead of deriving state from event counts.

Durable tables are `order_import_pool`, `order_recognition_batches`, `order_import_logs`, `orders`, and `order_items`. Legacy `import_batches` and `import_tasks` remain only for schema compatibility and are not order-import APIs.

### 3. Contracts

#### Provider settings

`orderRecognitionProvider` is one of `ollama`, `openai`, or `aliyun_batch`.

- `ollama`: realtime `/api/chat`, no API key.
- `openai`: realtime OpenAI-compatible `/chat/completions`, optional bearer API key.
- `aliyun_batch`: manual OpenAI-compatible File + Batch workflow, required bearer API key.

`UpdateSettingsRequest` carries provider-specific base URLs, models, optional replacement keys, clear-key flags, `aliyunEnableThinking`, and global realtime `maxConcurrency` in `1..=8`. Secrets live only in versioned application-data `settings.toml`; public `AppSettings` exposes masked/configured fields and never plaintext keys. Saving settings must wake the realtime queue when a realtime provider becomes active and must always resume existing remote batches, even when the selected provider is no longer `aliyun_batch`.

#### Pool and logs

`OrderImportPool` contains `{ tasks, batches, pending, processing, failed }`. Task states are:

```text
pending -> processing -> removed on success
pending -> batch_submitting -> batch_locked -> removed on success
processing | batch_submitting | batch_locked -> failed on local/remote failure or cancellation remainder
failed -> pending on explicit retry
```

Only `pending` and `failed` pool tasks may be individually deleted. A successful recognition transaction creates the order/items, transfers image metadata to `orders`, inserts one immutable `order_import_logs` row, and removes the pool row. The transaction is idempotent by task ID: replayed remote output must not duplicate the order or completion log.

`OrderImportLogsQuery` defaults page to `1`, page size to `20`, clamps page size to `1..=100`, and sorts by `completed_at DESC, task_id DESC`. Logs never change after insertion. Editing or deleting an order must not update or remove its log. Logs do not own images and do not claim source hashes.

#### Realtime recognition

Ollama and generic OpenAI-compatible providers consume every `pending` task from the same dynamic process-wide queue. New imports wake the queue while older work is in flight. Each drain re-reads current private settings and respects the current global concurrency ceiling. Switching providers prevents new work from starting under the old provider; already-sent requests may finish.

Ollama sends `stream: false`, JSON Schema in `format`, `temperature: 0`, and `num_ctx: 16384`. OpenAI-compatible recognition sends a base64 data URL, `response_format.type: json_schema`, a strict schema, and bearer auth when configured. Both normalize the same result:

```json
{
  "order_number": "string|null",
  "product_names": ["string"],
  "paid_amount": "number|null",
  "ordered_at": "string|null",
  "shop_name": "string|null"
}
```

Store money as integer fen and valid local time as `YYYY-MM-DD HH:mm`. Invalid optional values become `null` plus a warning; preserve the provider JSON in `raw_ocr_json`.

#### Aliyun batch recognition

`submit_order_recognition_batch` atomically locks all currently `pending` tasks, creates a local snapshot with an incrementing short number such as `B000001`, and returns its local ID. New imports remain pending and another submission may create a concurrent batch while earlier batches run.

Build one JSONL request per eligible task using `custom_id = task.id`, `POST /v1/chat/completions`, the strict vision schema, and an inline base64 image. Enforce 50,000 requests, 500 MB per input file, and 6 MB per line. Reject an invalid task into `failed` without preventing valid tasks in the same submission from running.

Persist checkpoints in this order:

1. Local batch and locked task membership.
2. Uploaded `input_file_id` immediately after file upload.
3. Remote `batch_id` immediately after create/list recovery.
4. Remote status and output/error file IDs while polling.
5. Idempotent per-task results, then terminal local batch status.

The local batch stores the submitted base URL, model, and thinking flag. Polling reads the current Aliyun key so a key can be repaired after restart without changing the submitted model snapshot.

#### Restart and cancellation

Migrations run before recovery queries. On startup, realtime `processing` tasks become `failed/app_interrupted`; `batch_submitting` and `batch_locked` tasks remain locked and their active local batches are supervised again.

- Remote ID present: resume retrieval directly; never upload or create again.
- Input file ID present but remote ID absent: list by input file and local recovery name, bind the existing remote batch if found, then continue polling. A persisted create-attempt checkpoint prevents immediate duplicate creation; after a bounded recovery grace period with no matching cloud job, create from the already uploaded file to cover a crash immediately before the original request was sent.
- No input file checkpoint: the cloud request could not have started; fail the local batch and release its tasks.
- Repeated output or a crash during finalization: replay must be harmless because success persistence is idempotent.

Cancellation calls the remote cancel endpoint before changing local state. A `cancelling` response is not terminal: keep the supervisor running until `cancelled`/`canceled`, reconcile any completed output first, then mark every remaining locked task `failed`, clear `recognition_batch_id`, and set the local batch to `canceled` in the same running application session. Never require another restart to release canceled tasks or their images.

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Empty image list or more than 500 selected images | Return `invalid_input`; copy and persist nothing |
| Unsupported extension, invalid magic bytes, or unreadable file | Reject before pool persistence |
| Same SHA-256 in the request, pool, or image-owning order | Count as skipped; create no pool/log row and make no OCR call |
| Realtime provider request/JSON failure | Mark only that task `failed`; require explicit retry |
| OpenAI/Aliyun key absent when required | Return actionable validation/service error without exposing any key |
| Submit batch under a non-Aliyun provider | Return `invalid_input`; keep pending tasks unchanged |
| Batch submission with no pending tasks | Return `invalid_input`; create no empty batch |
| One JSONL line is invalid/over 6 MB | Fail that task; submit remaining valid tasks |
| Remote batch transient retrieval/download failure | Keep local batch active and retry with backoff |
| Process exits during realtime request | Mark that task failed at startup; do not auto-retry it |
| Process exits during remote batch | Preserve lock and checkpoints; resume supervision without duplicate upload/order |
| Cancel endpoint fails | Leave local batch active and locked; surface the error |
| Cancel endpoint returns `cancelling` | Continue polling; do not release tasks prematurely |
| Remote cancellation becomes terminal | Import completed results, fail/unlock remainder immediately in the current process |
| Delete `processing`, `batch_submitting`, or `batch_locked` task | Return `invalid_input`; preserve row and image |
| Delete pending/failed task database failure | Restore its temporarily moved image |
| Delete a recognized order | Remove its owned image and hash claim, preserve immutable completion log |
| Duplicate remote result or finalization replay | Create at most one order and one log per task |

### 5. Good/Base/Bad Cases

- Good: three realtime imports are running, a fourth import arrives, and the same queue starts it as capacity becomes available without creating or selecting a user-visible batch.
- Good: `B000001` and `B000002` run concurrently while new pending screenshots remain editable; restarting resumes both from persisted cloud IDs.
- Good: after restart, cancel returns `cancelling`, the supervisor observes `cancelled`, imports completed lines, and immediately releases every remainder to `failed` without a second restart.
- Base: a remote result omits an optional date; the valid fields still produce an order and immutable log with a warning.
- Bad: storing cloud IDs only in memory, unlocking on the first cancel response, starting one queue per import click, auto-retrying failures forever, mutating logs when orders change, or generating a new task ID during retry violates this contract.

### 6. Tests Required

- Migration: fresh file, m0004 upgrade with successful and incomplete legacy tasks, and interrupted m0005 recovery.
- Startup recovery: realtime `processing` becomes failed while remote batch locks and checkpoints survive.
- Pool database: concurrent batch allocation locks only each submission's pending snapshot; success is atomic and idempotent.
- Realtime queue: measure peak in-flight work and assert it never exceeds current configured concurrency.
- Protocol mocks: assert Ollama and OpenAI-compatible paths, bearer auth, image data URL, strict schema, and normalized response.
- Batch JSONL: assert `custom_id`, endpoint, model/thinking flag, strict schema, and isolation of unreadable/oversize inputs.
- Remote restart: bind an already accepted remote batch from persisted input file ID and assert no duplicate upload/create/order.
- Remote cancellation regression: restart with locked work, receive `cancelling` then `cancelled`, and assert pool task is failed/unlocked and local batch canceled before another restart.
- File compensation: failed pool deletion/import/order deletion restores ownership consistently and releases hashes only on success.
- Frontend: order task page dual-pane states, manual submit/cancel controls, retry/delete rules, logs pagination, and event-driven reload.
- Quality gate: `cargo fmt --check`, strict Clippy, full Rust tests, `bun run lint`, `bun run test`, and `bun run build`.

### 7. Wrong vs Correct

#### Wrong

```rust
// The remote ID disappears when the process exits.
tokio::spawn(poll_remote(create_batch(input_file_id).await?.id));
```

```rust
// A cancel request was accepted, but the cloud job is still cancelling.
database.release_all_tasks(batch_id).await?;
```

#### Correct

```rust
database.set_input_file_id(batch_id, &input_file_id).await?;
let remote = client.create(&settings, &input_file_id, batch_id).await?;
database.set_remote_batch_id(batch_id, &remote.id).await?;
supervisor.poll_until_terminal(batch_id).await?;
```

```rust
let remote = client.cancel(base_url, api_key, remote_id).await?;
database.mark_batch_canceling(batch_id).await?;
if terminal_status(&remote.status) {
    supervisor.finalize(batch_id, remote).await?;
} else {
    supervisor.poll_until_terminal(batch_id).await?;
}
```

Persist each external checkpoint before advancing local state, use a per-process batch poll guard, and make finalization idempotent. This is what makes restart recovery and cancellation release correct.
