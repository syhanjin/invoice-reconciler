# Project Workspace Contract

## 1. Scope / Trigger

This contract applies to Project navigation, project list/detail actions, invoice-ledger membership controls, project filtering, ordering, export directory selection, overwrite confirmation, and cross-workspace refresh.

## 2. Signatures

Components use only the typed wrappers in `src/lib/api.ts`:

```ts
api.listProjects(query: { ..., archived?: boolean }): Promise<ProjectsPageData>
api.getProject(id): Promise<ProjectDetail>
api.createProject(name): Promise<ProjectDetail>
api.createProjectAndAssignInvoices(name, invoiceIds): Promise<ProjectDetail>
api.renameProject(id, name): Promise<ProjectDetail>
api.deleteProject(id): Promise<void>
api.archiveProject(id): Promise<void>
api.unarchiveProject(id): Promise<void>
api.assignInvoicesToProject(invoiceIds, projectId, acceptReassignment): Promise<AssignInvoicesResult>
api.reorderProjectInvoices(projectId, invoiceIds): Promise<ProjectDetail>
api.exportProject(id, outputDirectory, overwrite): Promise<ExportProjectResult>
```

Project summaries/details expose nullable `archivedAt`. Ledger rows expose backend-provided `matchStatus` plus nullable `projectId`/`projectName`.

## 3. Contracts

- Project is a first-level operational workspace. A segmented active/archived view sends `archived: false|true`; both views support search, paging, detail, current valid actual amount, export status, and delete.
- Project detail renders backend-provided ordered members, match state, order/product summary, invoice/order/actual amounts, and the concrete blocking reason. Frontend must not recompute export eligibility or amounts.
- Active project detail supports rename, archive, and reorder. Move controls use Lucide icon buttons with tooltips, disable unavailable directions, and send the complete reordered ID list. A failed reorder reloads authoritative detail.
- Archived detail displays archive state/time and allows only view, export, delete, and unarchive. Rename and reorder controls are hidden or disabled; archive/unarchive refreshes order, invoice, matching, and project projections through the shared invalidation atom.
- Deleting a project requires confirmation and explains that only membership is removed. It must not imply invoices, orders, matches, or files are deleted.
- Export is disabled when `exportable` is false and the backend reason stays visible. Eligible export opens a directory picker, never a free-form path field. A `destination_exists` response opens a second explicit overwrite dialog and reuses the selected directory with `overwrite: true`.
- The invoice ledger displays nullable project membership and filters by all, assigned, unassigned, or one active project. Query changes clear page selection and reset page one.
- Selected invoice rows expose assignment only when all selected rows are unassigned. Existing project targets use `acceptReassignment: false`; “新建项目” creates and assigns atomically. There is no invoice-ledger remove or reassignment confirmation flow.
- Membership changes clear selection, reload invoice/project projections, and show one result toast. Jotai refresh atoms are invalidation tokens, not durable membership state.

## 4. Validation & Error Matrix

| Condition | Frontend behavior |
| --- | --- |
| Blank project name | Disable save; backend remains authoritative for trimming/uniqueness |
| Project list/detail request fails | Show Sonner error and avoid inventing local data |
| No projects exist | Keep “新建项目” available in the assignment target Select |
| Any selected invoice has membership | Disable assignment before opening the dialog |
| Archived detail is selected | Hide rename; disable reorder; keep export/delete/unarchive available |
| Reorder rejects stale members | Show error and reload project detail |
| Project is blocked | Disable export and display `blockingReason` |
| Directory picker canceled | Invoke no export command |
| Backend returns `destination_exists` | Ask before calling export again with overwrite enabled |
| Project delete confirmed | Reload list; do not mutate invoice/order entities in React |

## 5. Good/Base/Bad Cases

- Good: select several ledger rows, assign to one project, then view the same membership and order in Project detail.
- Good: archive a project, continue to inspect/export it in the archived view, then unarchive and recover the same ordered members.
- Base: an empty or partially matched project remains searchable/editable while export is visibly unavailable.
- Bad: showing archived projects as editable, offering ledger reassignment/removal, computing `Math.min` in React as export authority, asking for a ZIP filename, or overwriting after one click violates this contract.

## 6. Tests Required

- Project-page tests: active/archived queries, detail selection, create/rename/archive/unarchive/delete confirmation, archived read-only controls, empty/blocked export state, move payload, directory cancel/select, `destination_exists`, and confirmed overwrite.
- Invoice-page tests: match/membership display, all/assigned/unassigned/specific query payloads, occupied-selection disablement, new-project atomic assignment, and absence of remove/reassignment controls.
- API/type checks must cover archive command names, camel-case payloads, nullable `archivedAt`/membership, match status, blocking reason, and `destination_exists` error-code extraction.
- Run `bun run lint`, `bun run test`, and `bun run build`; inspect the project split view at the Tauri minimum `920x640` and default `1360x840` sizes.

## 7. Wrong vs Correct

Wrong:

```ts
if (invoice.confirmed) total += Math.min(invoice.totalFen!, orderTotal)
await api.exportProject(project.id, userTypedPath, true)
```

Correct:

```ts
const project = await api.getProject(id)
if (project.exportable) {
  const directory = await open({ directory: true, multiple: false })
  if (typeof directory === "string") await api.exportProject(id, directory, false)
}
```

Wrong: clear a project's detail locally after archive and assume other workspaces will converge on their own.

Correct: increment the shared workspace invalidation token after archive/unarchive so every affected server projection reloads.
