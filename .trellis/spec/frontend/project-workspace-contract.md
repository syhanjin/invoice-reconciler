# Project Workspace Contract

## 1. Scope / Trigger

This contract applies to Project navigation, project list/detail actions, invoice-ledger membership controls, project filtering, ordering, export directory selection, overwrite confirmation, and cross-workspace refresh.

## 2. Signatures

Components use only the typed wrappers in `src/lib/api.ts`:

```ts
api.listProjects(query): Promise<ProjectsPageData>
api.getProject(id): Promise<ProjectDetail>
api.createProject(name): Promise<ProjectDetail>
api.renameProject(id, name): Promise<ProjectDetail>
api.deleteProject(id): Promise<void>
api.assignInvoicesToProject(invoiceIds, projectId, acceptReassignment): Promise<AssignInvoicesResult>
api.removeInvoicesFromProjects(invoiceIds): Promise<void>
api.reorderProjectInvoices(projectId, invoiceIds): Promise<ProjectDetail>
api.exportProject(id, outputDirectory, overwrite): Promise<ExportProjectResult>
```

`api.listInvoices` accepts `projectId` or `unassignedOnly`, and ledger rows expose nullable `projectId`/`projectName`.

## 3. Contracts

- Project is a first-level operational workspace. Its list supports search, paging, create, rename, delete, current valid actual amount, and export status.
- Project detail renders backend-provided ordered members, match state, order/product summary, invoice/order/actual amounts, and the concrete blocking reason. Frontend must not recompute export eligibility or amounts.
- Move controls use Lucide icon buttons with tooltips, disable unavailable directions, and send the complete reordered ID list. A failed reorder reloads authoritative detail.
- Deleting a project requires confirmation and explains that only membership is removed. It must not imply invoices, orders, matches, or files are deleted.
- Export is disabled when `exportable` is false and the backend reason stays visible. Eligible export opens a directory picker, never a free-form path field. A `destination_exists` response opens a second explicit overwrite dialog and reuses the selected directory with `overwrite: true`.
- The invoice ledger displays nullable project membership and filters by all, unassigned, or one project. Query changes clear page selection and reset page one.
- Selected invoice rows expose assign and remove actions. Unconfirmed invoices remain assignable. A first assignment response with conflicts writes nothing and opens a second confirmation listing source projects; only then send `acceptReassignment: true`.
- Membership changes clear selection, reload invoice/project projections, and show one result toast. Jotai refresh atoms are invalidation tokens, not durable membership state.

## 4. Validation & Error Matrix

| Condition | Frontend behavior |
| --- | --- |
| Blank project name | Disable save; backend remains authoritative for trimming/uniqueness |
| Project list/detail request fails | Show Sonner error and avoid inventing local data |
| No projects exist | Keep assignment confirmation disabled and direct the user to create one |
| Assignment returns conflicts | Keep selection/project target and require explicit reassignment confirmation |
| Reorder rejects stale members | Show error and reload project detail |
| Project is blocked | Disable export and display `blockingReason` |
| Directory picker canceled | Invoke no export command |
| Backend returns `destination_exists` | Ask before calling export again with overwrite enabled |
| Project delete confirmed | Reload list; do not mutate invoice/order entities in React |

## 5. Good/Base/Bad Cases

- Good: select several ledger rows, assign to one project, then view the same membership and order in Project detail.
- Good: choosing an occupied invoice shows its source project and only the second action performs atomic reassignment.
- Base: an empty or partially matched project remains searchable/editable while export is visibly unavailable.
- Bad: computing `Math.min` in React as export authority, changing project membership from two unrelated entry points, asking for a ZIP filename, or overwriting after one click violates this contract.

## 6. Tests Required

- Project-page tests: list/detail selection, create/rename/delete confirmation, empty/blocked export state, move payload, directory cancel/select, `destination_exists`, and confirmed overwrite.
- Invoice-page tests: membership display, all/unassigned/specific query payloads, bulk assignment, remove confirmation, conflict source display, and second-step reassignment acceptance.
- API/type checks must cover command names, camel-case payloads, nullable membership, blocking reason, and `destination_exists` error-code extraction.
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
