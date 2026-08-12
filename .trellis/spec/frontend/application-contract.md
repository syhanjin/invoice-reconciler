# Desktop Application Contract

## Scenario: Orders Workspace And Tauri Boundary

### 1. Scope / Trigger

This contract applies when changing the React shell, orders table, import progress, order editor, settings, Tauri invoke wrappers, or the `import-task-updated` listener. The first screen is the operational orders workspace, not a landing page.

### 2. Signatures

All backend access lives in `src/lib/api.ts`. Components call these typed wrappers rather than importing `invoke` or `listen` directly:

```ts
api.bootstrap(): Promise<BootstrapState>
api.checkOllama(settings: AppSettings): Promise<OllamaStatus>
api.createImportBatch(imagePaths: string[]): Promise<BatchDetail>
api.listImportBatches(query: ImportBatchesQuery): Promise<ImportBatchesPage>
api.getBatch(id: string): Promise<BatchDetail>
api.listOrders(query: OrdersQuery): Promise<OrdersPage>
api.getOrder(id: string): Promise<OrderDetail>
api.updateOrder(request: UpdateOrderRequest): Promise<OrderDetail>
api.deleteOrder(id: string): Promise<void>
api.listenForTaskUpdates(handler): Promise<UnlistenFn>
```

Rust serializes public structs with `camelCase`. Tauri command names remain snake case.

### 3. Contracts

- `settingsAtom` and `activeBatchAtom` cache current projections; SQLite remains authoritative.
- A task event carries the entire updated `BatchDetail`. Replace the active batch snapshot when its ID matches rather than incrementing counters locally.
- Import tasks live in a separate first-level workspace. Its order tab pages persisted batch summaries with `20`, `50`, or `100` rows and loads full tasks only after selecting a batch. The invoice tab remains disabled until that domain is implemented.
- A successful order screenshot import navigates to the import-task workspace and selects the new batch. Canceling the file dialog or rejecting batch creation preserves the orders workspace.
- A live event may refresh the matching selected detail and summary, but an event for another batch must not replace or reload the detail being inspected.
- Refresh the orders query after task success and after an order mutation so persisted values remain authoritative.
- The orders query is server-paged and contains page, page size, search, duplicate-only filter, sort field, and sort direction.
- The orders table controls `page`, `pageSize`, `search`, `duplicateOnly`, `sortBy`, and `sortDirection` in `OrdersPage`; TanStack Table renders the current server page and must not apply client-side sorting or pagination.
- Search, duplicate-only, sort, and page-size changes reset to page `1`. If a response makes the current page invalid, request the last valid page. Empty results remain on page `1` of `1`.
- Persist only table presentation preferences under a versioned local-storage key: page size, sort, column visibility, constrained column sizing, and comfortable/compact density. Normalize unknown columns and invalid values on read. Never persist search, duplicate-only, current page, or the open order.
- The order-number column is always visible. Optional columns may be hidden, table preferences may be reset together, and resize/header controls must not activate a data row.
- Order forms use React Hook Form plus Zod. Convert yuan display input to integer fen before invoking the backend.
- Raw OCR JSON is read-only. Product order in the submitted array is significant.
- Use `convertFileSrc` only for the absolute path returned by `get_order`; do not construct filesystem or asset URLs in components.
- Keep controls based on shadcn/ui and Lucide icons. Do not add custom SVG controls or a second component framework.

### 4. Validation & Error Matrix

| Condition | Frontend behavior |
| --- | --- |
| No selected image | Keep import disabled; do not invoke |
| Native dialog canceled | Preserve current batch and table state |
| Backend invoke rejects | Convert the value through `errorMessage` and show a Sonner error toast |
| Orders reload rejects | Preserve the last successful page, end the loading state, and show the Sonner error |
| Stored table preference is malformed or obsolete | Drop unknown fields and normalize page size, sort, density, visibility, and column widths to supported values |
| Requested orders page exceeds the new last page | Change to the last valid page and issue one corrected server request |
| Settings URL lacks HTTP(S), model is empty, or concurrency outside `1..=8` | Block submit with form validation |
| Ollama reachable but model missing | Show the backend pull guidance; never trigger installation or pull |
| Empty amount/date/order number/shop | Submit `null`, not an empty-string sentinel |
| Live event for another batch | Do not replace the visible active batch |
| Import-history or batch-detail request fails | Clear stale content, show a retryable page state, and display the Sonner error |
| Import-history page becomes empty | Select no detail and show the order-task empty state |
| Component unmount | Call the `UnlistenFn` to remove the Tauri event listener |
| Delete confirmation declined | Perform no command and preserve the open record |

### 5. Good/Base/Bad Cases

- Good: a task event replaces the matching batch snapshot, the task center updates its selected detail and summary, and successful OCR causes the server-paged order table to refresh.
- Good: sorting a supported header resets to page one, requests the backend sort field, and retains column layout and density preferences after remount.
- Base: a small window switches task history/detail to stacked sections while keeping their independent scroll areas and actions accessible.
- Bad: issuing SQL in React, client-sorting only the visible server page, persisting a transient search/page selection, loading every historical task into one response, replacing selected history with an unrelated live event, duplicating backend queue counters in component state, importing Tauri `invoke` throughout the component tree, or displaying a managed path directly as an image URL.

### 6. Tests Required

- Unit tests for currency/date presentation and yuan-to-fen conversion boundaries.
- Jotai store test asserting that an event for the active batch replaces the stored snapshot.
- API contract test or TypeScript build asserting camel-case invoke argument names.
- Component tests for settings validation and nullable order edit fields when those flows change.
- Component tests for all order sort-field mappings and sort cycles; search/filter/page-size reset; first/previous/next/last and validated page jumps; invalid-page correction; column visibility/resizing; density; reset; preference normalization; and reload failure preserving successful rows.
- Component tests for task-history pagination, disabled invoice tab, detail loading/error retry, and post-import navigation when those flows change.
- Desktop smoke tests at the default and minimum window sizes for table, task-center history/detail scrolling, image preview, edit/delete sheet, and settings.
- Always run `bun run build`, `bun run lint`, and `bun run test` before commit.

### 7. Wrong vs Correct

#### Wrong

```ts
await invoke("update_order", {
  id,
  paid_amount: amount,
})
```

This bypasses the shared boundary and does not match the Rust request wrapper.

#### Correct

```ts
await api.updateOrder({
  id,
  paidAmountFen: yuanToFen(amount),
  productNames,
  externalOrderNumber: emptyToNull(orderNumber),
  orderedAt: emptyToNull(orderedAt),
  shopName: emptyToNull(shopName),
})
```

Keep invoke payload construction in `api.ts` and pass the entire typed `request` object expected by the command.

Do not use a `BatchDetail[]` history response. Use `ImportBatchSummary[]` for the page and `api.getBatch(id)` for the selected detail.

For orders, do not pass the current server page through a TanStack sorted row model. Update the controlled `OrdersQuery` and reload through `api.listOrders` instead.
