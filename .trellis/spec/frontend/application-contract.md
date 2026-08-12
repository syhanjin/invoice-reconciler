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
- Refresh the orders query after task success and after an order mutation so persisted values remain authoritative.
- The orders query is server-paged and contains page, page size, search, duplicate-only filter, sort field, and sort direction.
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
| Settings URL lacks HTTP(S), model is empty, or concurrency outside `1..=8` | Block submit with form validation |
| Ollama reachable but model missing | Show the backend pull guidance; never trigger installation or pull |
| Empty amount/date/order number/shop | Submit `null`, not an empty-string sentinel |
| Live event for another batch | Do not replace the visible active batch |
| Component unmount | Call the `UnlistenFn` to remove the Tauri event listener |
| Delete confirmation declined | Perform no command and preserve the open record |

### 5. Good/Base/Bad Cases

- Good: a task event replaces the batch snapshot, the progress panel updates every status count, and successful OCR causes the server-paged table to refresh.
- Base: a small window keeps the table scrollable and the order sheet within the viewport while all commands remain accessible.
- Bad: issuing SQL in React, duplicating backend queue counters in component state, importing Tauri `invoke` throughout the component tree, or displaying a managed path directly as an image URL.

### 6. Tests Required

- Unit tests for currency/date presentation and yuan-to-fen conversion boundaries.
- Jotai store test asserting that an event for the active batch replaces the stored snapshot.
- API contract test or TypeScript build asserting camel-case invoke argument names.
- Component tests for settings validation and nullable order edit fields when those flows change.
- Desktop smoke tests at the default and minimum window sizes for table, progress panel, image preview, edit/delete sheet, and settings.
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
