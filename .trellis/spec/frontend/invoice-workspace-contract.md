# Invoice Workspace Contract

## Scenario: Invoice Import And Ledger

### 1. Scope / Trigger

This contract applies to the invoice navigation item, invoice import dialog, invoice task tab, review dialog, ledger table, invoice detail sheet, settings credentials, PDF.js preview, and invoice Tauri wrappers.

### 2. Signatures

Components use typed functions in `src/lib/api.ts`:

```ts
api.prepareInvoiceBatch(filePaths, forcedType): Promise<InvoiceBatchPreview>
api.createInvoiceBatch(preflightToken): Promise<InvoiceBatchDetail>
api.listInvoiceBatches(query): Promise<InvoiceBatchesPage>
api.getInvoiceBatch(id): Promise<InvoiceBatchDetail>
api.retryInvoiceTask(id, forcedType): Promise<InvoiceBatchDetail>
api.confirmInvoiceSplit(taskId, groups): Promise<InvoiceBatchDetail>
api.listInvoices(query: {
  ...,
  matchStatus?: "unmatched" | "matched" | "confirmed"
  projectStatus?: "assigned" | "unassigned"
  projectId?: string
}): Promise<InvoicesPage>
api.getInvoice(id): Promise<InvoiceDetail>
api.updateInvoice(request): Promise<InvoiceDetail>
api.deleteInvoice(id): Promise<void>
api.deleteInvoices(ids): Promise<DeleteInvoicesResult>
api.listenForInvoiceTaskUpdates(handler): Promise<UnlistenFn>
```

### 3. Contracts

- The first invoice screen is the operational ledger with import, server paging, filters, sorting, row selection, and record details.
- Selecting files always invokes preflight first. The confirmation dialog displays valid, duplicate, invalid, predicted-request, and PDF-page counts. Only its explicit confirm action invokes create.
- The invoice task tab is enabled and uses separate invoice batch state/events. A retry type is optional and may differ from the original batch type.
- Review group numbers map every OCR result to one group. Equal values merge, distinct values split, and the dialog states that confirmation does not call OCR again.
- Ledger query state owns page, page size `20|50|100`, committed search, subtype, date range, possible-duplicate filter, sort field, and direction. Query-changing controls clear selection and reset page one.
- Ledger query state also owns match status, all/assigned/unassigned membership, and specific active-project filters. Rows display backend-provided match/membership badges and low-contrast status backgrounds; React never derives match state from visible rows.
- Selected rows can be assigned only when every selected invoice is currently unassigned. If any selected row has a `projectId`, disable assignment. The invoice workspace exposes no reassignment or remove-from-project action.
- The assignment target Select starts with “新建项目”. That path asks for a project name and calls `createProjectAndAssignInvoices`; existing targets call normal assignment with reassignment acceptance disabled. Both paths are backend-transactional.
- Selection is limited to the current server page. Bulk delete sends one request, reports partial results, clears successful IDs, retains failed visible IDs, and refreshes the ledger.
- Detail fields are editable through React Hook Form/Zod; yuan strings become integer fen. Raw Tencent JSON is read-only.
- Absolute preview paths come only from `get_invoice`. Images use `convertFileSrc`; PDFs render inside the sheet through `pdfjs-dist` and its bundled worker.
- Settings never receive the stored SecretKey. Empty credential inputs preserve existing values; SecretId and SecretKey must be entered together; explicit clear removes both.

### 4. Validation & Error Matrix

| Condition | Frontend behavior |
| --- | --- |
| Native file dialog canceled | Do not preflight or change navigation |
| Preflight returns invalid/duplicate files | Show counts and details; allow valid siblings to proceed |
| Create rejects due to expired token/change | Show error and require a new selection/preflight |
| Awaiting review | Show review action; retry is not offered |
| Failed/interrupted task without source | Hide retry action |
| Invoice page becomes out of range | Request the last valid page |
| Any selected invoice already has a project | Disable the assignment action; offer no move/remove bypass |
| New-project name is blank | Disable confirmation; backend still validates trimmed uniqueness |
| Bulk delete returns mixed results | Report both counts; keep only failed visible selection |
| SecretId or SecretKey entered alone | Block settings submit |
| Permission warning exists | Keep a visible security alert on Settings |
| Preview fails | Show a contained error state; keep invoice fields usable |

### 5. Good/Base/Bad Cases

- Good: file selection opens preflight confirmation, then one create command starts the paid work and navigates to invoice tasks.
- Good: selecting a subtype changes the server query and page; it does not filter only the currently rendered rows.
- Good: select unassigned invoices, choose “新建项目”, enter a name, and create the project with all memberships atomically.
- Base: a batch with successes, failures, and review items presents each state and permits only valid actions.
- Bad: enabling assignment for a mixed occupied/unassigned selection, exposing a ledger remove button, computing match state from the current page, hiding request counts, starting OCR directly from file selection, constructing asset paths in React, rendering PDFs with an external viewer, or storing SecretKey in Jotai/local storage.

### 6. Tests Required

- Import test proving preflight precedes create and cancel creates nothing.
- Query tests for subtype/date/duplicate filters, supported sorts, page reset, and page correction.
- Ledger-status tests for match/membership query payloads, badge/background rendering, occupied-selection assignment disablement, absence of the remove action, and new-project create-and-assign.
- Selection and bulk-delete tests covering confirmation and partial failure.
- Review tests covering split/merge group payload and no retry path.
- Settings tests covering paired credentials, preserve-on-empty, explicit clear, masked display, and permission warning.
- Detail tests for nullable fields, yuan/fen conversion, raw response read-only, delete confirmation, image preview, and PDF rendering error state.
- Run `bun run lint`, `bun run test`, and `bun run build`.

### 7. Wrong vs Correct

#### Wrong

```ts
await api.createInvoiceBatch(await openInvoiceFiles())
```

This hides validation and request cost from the user.

#### Correct

```ts
const preview = await api.prepareInvoiceBatch(paths, forcedType)
setPreview(preview)
// Only the confirmation button calls createInvoiceBatch(preview.preflightToken).
```

Keep the paid operation behind an explicit confirmation using the single-use server token.

Wrong: let a mixed selection enter the assignment dialog and resolve membership conflicts after submission.

Correct: disable assignment as soon as any selected visible row has a project, and let membership removal/reorganization happen only in the active project workspace.
