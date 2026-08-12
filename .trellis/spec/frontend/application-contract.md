# Desktop Application Contract

## Scenario: Orders Workspace And Tauri Boundary

### 1. Scope / Trigger

This contract applies when changing the React shell, unified order import pool, immutable completion log, orders table, order editor, recognition settings, Tauri invoke wrappers, or `order-import-task-updated` listener. The first screen is the operational orders workspace, not a landing page. Invoice import remains a separate batch-based tab and event domain.

### 2. Signatures

All backend access lives in `src/lib/api.ts`. Components call typed wrappers rather than importing `invoke` or `listen` directly:

```ts
api.bootstrap(): Promise<BootstrapState>
api.checkOrderRecognizer(request: SaveSettingsRequest): Promise<RecognizerStatus>
api.importOrderImages(imagePaths: string[]): Promise<ImportOrderImagesResult>
api.getOrderImportPool(): Promise<OrderImportPool>
api.listOrderImportLogs(query): Promise<OrderImportLogsPage>
api.retryOrderImportTask(id: string): Promise<boolean>
api.retryFailedOrderImportTasks(): Promise<number>
api.deleteOrderImportTask(id: string): Promise<void>
api.submitOrderRecognitionBatch(): Promise<string>
api.cancelOrderRecognitionBatch(id: string): Promise<void>
api.listOrders(query: OrdersQuery): Promise<OrdersPage>
api.getOrder(id: string): Promise<OrderDetail>
api.updateOrder(request: UpdateOrderRequest): Promise<OrderDetail>
api.deleteOrder(id: string): Promise<void>
api.deleteOrders(ids: string[]): Promise<DeleteOrdersResult>
```

Rust serializes public structs with `camelCase`; Tauri command names remain snake case. `BootstrapState` contains settings and the latest invoice batch only. Do not restore `latestBatch`, order `BatchDetail`, order batch summaries, or order batch selection state.

### 3. Contracts

#### Unified task workspace

- A successful file selection calls `importOrderImages` directly and navigates to the import-task workspace. There is no user-visible order import batch creation or batch picker.
- The order tab uses a two-column desktop layout: the left side is the mutable pool, the right side is immutable completion history. Narrow layouts expose the same two panes as nested tabs with independent scroll areas.
- The left pane shows counts and rows for `pending`, `processing`, `batch_submitting`, `batch_locked`, and `failed`. Failed rows expose retry; only pending/failed rows expose delete, with destructive confirmation.
- When `settings.orderRecognitionProvider === "aliyun_batch"`, expose manual “submit batch” for current pending tasks. Show every active batch concurrently with its short number, task count, model, status, and cancel control. New imports must remain usable while batches run.
- Realtime providers do not expose manual submit; newly imported pending tasks are consumed by the backend queue automatically.
- The right pane pages `20`, `50`, or `100` immutable `OrderImportLog` rows. It displays provider/model, optional short batch number/warning, and completion time. It does not offer edit/delete actions because order mutations never rewrite logs.

`order-import-task-updated` is an invalidation event. Increment the order refresh token and reload pool/log projections; do not increment local counters, infer task transitions, or reuse `invoice-task-updated`. Successful recognition refreshes the server-paged orders query.

#### Recognition settings

Settings use a provider Select with values `ollama`, `openai`, and `aliyun_batch`, then render only the relevant base URL/model/key/thinking controls. API key fields preserve an existing key when blank, support explicit clearing, and show only configured/masked state returned by the backend. “Check service” validates the currently selected provider through `checkOrderRecognizer`.

Ollama and OpenAI-compatible providers share the realtime concurrency setting. Aliyun is manual batch mode and may enable thinking. Changing the selected provider does not hide active Aliyun batches from the task pool.

#### Orders table and mutations

- The orders query is server-paged and contains page, page size, search, duplicate-only filter, sort field, and sort direction.
- TanStack Table renders the current server page and must not apply client-side sorting or pagination.
- Search, duplicate-only, sort, and page-size changes reset to page `1`. If a response makes the current page invalid, request the last valid page.
- Persist only versioned table presentation preferences: page size, sort, column visibility/sizing, and density. Do not persist search, duplicate-only, page, or open order.
- Row selection is controlled by the orders page and scoped to the loaded server page. Bulk deletion uses one confirmation and one `deleteOrders` call, preserving failed selections and refreshing SQLite projections.
- Order forms use React Hook Form and Zod; convert yuan input to integer fen. Raw OCR JSON is read-only and product array order is significant.
- Use `convertFileSrc` only for the absolute path returned by `getOrder`.
- Use existing shadcn/ui and Lucide controls; do not add native selects, custom SVG controls, or a second component framework.

### 4. Validation & Error Matrix

| Condition | Frontend behavior |
| --- | --- |
| Native image dialog canceled | Preserve workspace state; do not invoke import |
| Import returns skipped duplicates | Report imported/skipped totals and navigate only when work was added |
| Backend invoke rejects | Convert through `errorMessage` and show Sonner error |
| Pool/log reload rejects | End loading, retain the last useful projection where possible, and show error |
| No pending tasks in Aliyun mode | Disable manual submit |
| Provider is realtime | Hide manual submit; keep active remote batch controls visible if any |
| Batch status is canceling | Disable repeated cancel while continuing to display the batch |
| Task is processing or batch locked | Expose neither retry nor individual delete |
| Delete confirmation declined | Perform no command and preserve row/image |
| Completion log is empty | Show an empty log state; page remains `1 / 1` |
| Log page becomes out of range | Request the new last valid page once |
| Settings URL/model/concurrency invalid | Block save/check with form validation |
| Required Aliyun key is absent | Show backend error; never expose stored plaintext key |
| Event arrives | Reload authoritative pool/log/order projections; do not derive counts from event payload |
| Component unmount | Call every Tauri `UnlistenFn` |
| Bulk order delete returns mixed results | Report successes/failures, retain failed visible selections, and reload orders |

### 5. Good/Base/Bad Cases

- Good: while `B000001` is running, the user imports more screenshots, sees them pending, submits `B000002`, and can cancel either batch independently.
- Good: after restart, a still-running batch remains visible; cancel stays disabled only during its cancel request/state, and tasks disappear or return failed as backend events invalidate the pool.
- Good: a completion event reloads the right log and orders table while leaving the log row immutable after later order edits/deletion.
- Base: on a narrow window the user switches between “任务池” and “导入完成” tabs without losing pagination or actions.
- Bad: rendering old order batch history/detail, disabling all imports while a batch runs, optimistic local unlock on cancel click, showing raw provider keys, using invoice events for orders, or calling Tauri `invoke` directly from components violates this contract.

### 6. Tests Required

- API/store tests: command names and camel-case arguments, absence of old order batch state, and order event invalidation behavior.
- Orders-page component tests: file import totals, duplicate reporting, task-center navigation, cancel behavior, and server-query refresh.
- Import-task component tests: desktop pool/log data, mobile tabs, all pool statuses, active concurrent batches, manual submit only in Aliyun mode, cancel, retry-all/single, delete confirmation, and log pagination.
- Settings component tests: provider selection, conditional fields, masked/preserved/cleared keys, Aliyun thinking, validation, and provider-aware service check.
- Existing order-table tests: every sort mapping/cycle, search/filter/page reset, invalid-page correction, presentation preferences, selection, and mixed bulk-delete results.
- Desktop smoke tests at default and minimum window sizes for dual-pane scrolling, responsive tabs, active batch controls, order preview/editor, and settings.
- Always run `bun run lint`, `bun run test`, and `bun run build` before commit.

### 7. Wrong vs Correct

#### Wrong

```ts
// Remote cancellation is asynchronous; this projection can be false.
setPool((pool) => ({ ...pool, tasks: pool.tasks.filter((task) => task.batchId !== id) }))
await api.cancelOrderRecognitionBatch(id)
```

```ts
// Reintroduces order-side batch selection and a stale snapshot.
const [activeBatch, setActiveBatch] = useAtom(activeBatchAtom)
```

#### Correct

```ts
await api.cancelOrderRecognitionBatch(id)
// The order-import event invalidates all affected projections.
setOrderRefreshToken((value) => value + 1)
```

```ts
const [pool, logs] = await Promise.all([
  api.getOrderImportPool(),
  api.listOrderImportLogs({ page, pageSize }),
])
```

The backend owns queue, remote, and cancellation state. The frontend issues typed commands and reloads durable projections rather than inventing transitions.
