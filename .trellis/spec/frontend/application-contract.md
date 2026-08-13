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
- The left pane shows counts and rows for `pending`, `processing`, and `failed`. Failed rows expose retry; only pending/failed rows expose delete, with destructive confirmation.
- Newly imported pending tasks are consumed by the backend realtime queue automatically. The order workspace has no submit/cancel Batch controls or active-batch projection.
- The right pane pages `20`, `50`, or `100` immutable `OrderImportLog` rows. It displays provider/model, warning, and completion time. Historical `provider = "aliyun_batch"` renders as `阿里云百炼（历史）` and has no actions.

`order-import-task-updated` is an invalidation event. Increment the order refresh token and reload pool/log projections; do not increment local counters, infer task transitions, or reuse `invoice-task-updated`. Successful recognition refreshes the server-paged orders query.

#### Recognition settings

Settings use a provider Select with values `ollama` and `openai`, then render only the relevant base URL/model/key controls. API key fields preserve an existing key when blank, support explicit clearing, and show only configured/masked state returned by the backend. “Check service” validates the currently selected provider through `checkOrderRecognizer`.

Ollama and OpenAI-compatible providers share the realtime concurrency setting. Do not expose Aliyun endpoint, key, thinking, submission, cancellation, or recovery controls.

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
| Task is processing | Expose neither retry nor individual delete |
| Delete confirmation declined | Perform no command and preserve row/image |
| Completion log is empty | Show an empty log state; page remains `1 / 1` |
| Log page becomes out of range | Request the new last valid page once |
| Settings URL/model/concurrency invalid | Block save/check with form validation |
| Event arrives | Reload authoritative pool/log/order projections; do not derive counts from event payload |
| Component unmount | Call every Tauri `UnlistenFn` |
| Bulk order delete returns mixed results | Report successes/failures, retain failed visible selections, and reload orders |

### 5. Good/Base/Bad Cases

- Good: imported screenshots enter the pool and transition to processing as realtime capacity becomes available.
- Good: a migrated failed task explains that Batch support was removed and exposes the normal retry action after realtime settings are ready.
- Good: a completion event reloads the right log and orders table while leaving the log row immutable after later order edits/deletion.
- Base: on a narrow window the user switches between “任务池” and “导入完成” tabs without losing pagination or actions.
- Bad: rendering order Batch history/detail, offering Aliyun settings, showing raw provider keys, using invoice events for orders, or calling Tauri `invoke` directly from components violates this contract.

### 6. Tests Required

- API/store tests: command names and camel-case arguments, absence of old order batch state, and order event invalidation behavior.
- Orders-page component tests: file import totals, duplicate reporting, task-center navigation, and server-query refresh.
- Import-task component tests: desktop pool/log data, mobile tabs, all realtime pool statuses, absence of Batch actions, retry-all/single, delete confirmation, historical Aliyun label, and log pagination.
- Settings component tests: exactly two provider options, conditional fields, masked/preserved/cleared OpenAI keys, validation, and provider-aware service check.
- Existing order-table tests: every sort mapping/cycle, search/filter/page reset, invalid-page correction, presentation preferences, selection, and mixed bulk-delete results.
- Desktop smoke tests at default and minimum window sizes for dual-pane scrolling, responsive tabs, order preview/editor, and settings.
- Always run `bun run lint`, `bun run test`, and `bun run build` before commit.

### 7. Wrong vs Correct

#### Wrong

```ts
// Removed runtime providers must not remain in the current settings union.
type OrderRecognitionProvider = "ollama" | "openai" | "aliyun_batch"
```

```ts
// Reintroduces order-side batch selection and a stale snapshot.
const [activeBatch, setActiveBatch] = useAtom(activeBatchAtom)
```

#### Correct

```ts
type OrderRecognitionProvider = "ollama" | "openai"
```

```ts
const [pool, logs] = await Promise.all([
  api.getOrderImportPool(),
  api.listOrderImportLogs({ page, pageSize }),
])
```

The backend owns queue state. The frontend issues typed commands and reloads durable projections rather than inventing transitions. Aliyun exists only as historical completion-log text.
