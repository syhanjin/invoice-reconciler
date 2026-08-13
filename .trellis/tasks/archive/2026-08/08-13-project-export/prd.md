# Project Management And Export Packages

## Goal

Add a project workspace where users organize invoices into named projects and export a reimbursement material package. A project contains multiple invoices; an invoice may be unassigned but belongs to at most one project. Export is allowed only when every project invoice has a confirmed order match and all required amounts and managed files are valid.

## Confirmed Product Decisions

- Projects are created, renamed, deleted, inspected, reordered, and exported from a new Project workspace.
- Invoice membership is maintained from the invoice ledger through bulk assign, remove, and confirmed cross-project reassignment actions. Project detail does not provide a second membership entry point.
- Unconfirmed invoices may be assigned. Project deletion only removes membership; it does not delete invoices, orders, matches, or files.
- Project names are trimmed, non-empty, at most 100 characters, and unique.
- Members have a durable manual order. New assignments append to the end. Excel rows, material folder indexes, and combined PDF groups share that order.
- Export does not lock or archive a project. Every export is regenerated from current durable state.
- Users select an output directory. The application creates a fixed safe filename and asks before overwriting an existing archive.

## Money And Eligibility

For each project invoice:

```text
invoice amount = invoices.total_fen
order amount = sum(confirmed orders.paid_amount_fen)
actual amount = min(invoice amount, order amount)
project actual amount = sum(member actual amounts)
```

- Money stays in integer fen until presentation.
- Every invoice must have one persisted confirmed match with at least one order.
- Invoice and order amounts must exist and be non-negative; invoices must be CNY.
- Each invoice preview and each matched order managed image must be readable.
- Backend export revalidates current SQLite data and files before writing anything. Any invalid member blocks the complete export and returns a concrete reason.

## Persistent Model

- Add `projects(id, name, created_at, updated_at)` with a unique project name.
- Add `project_invoices(project_id, invoice_id, position)` with globally unique `invoice_id`, project-local unique `position`, and cascading membership cleanup when either parent is deleted.
- Use a new repeatable SeaORM migration appended after `m0006` and typed SeaORM entities/repositories.
- Batch assignment/reassignment, removal, and complete-order replacement are transactional. Reassignment first returns conflicts without writing and only moves members after explicit acceptance.
- Removing members compacts project positions. Reordering rejects duplicate, missing, extra, or stale member sets.

## Backend Interfaces

Add typed camelCase request/response models and Tauri commands:

```text
list_projects(query) -> ProjectsPage
get_project({ id }) -> ProjectDetail
create_project({ name }) -> ProjectDetail
rename_project({ id, name }) -> ProjectDetail
delete_project({ id }) -> ()
assign_invoices_to_project({ invoiceIds, projectId, acceptReassignment }) -> AssignInvoicesResult
remove_invoices_from_projects({ invoiceIds }) -> ()
reorder_project_invoices({ projectId, invoiceIds }) -> ProjectDetail
export_project({ id, outputDirectory, overwrite }) -> ExportProjectResult
```

- Project summaries expose invoice/confirmed/unconfirmed counts, actual-amount sum for currently valid confirmed members, `exportable`, and a primary blocking reason.
- Project detail returns ordered members with invoice identity, match state, matched order summaries, the three amount values, file availability, and blocking reason.
- Assignment preview returns conflicting invoice and source-project identities when explicit reassignment is required.
- Export returns final path, invoice count, and actual amount sum.
- Add project membership to invoice ledger rows and project filtering to the invoice list query.

## Export Package

Generate `{safe project name}_{actual yuan with two decimals}.zip` with UTF-8 entry names and Deflate compression. ZIP root contains:

```text
发票明细.xlsx
组合材料.pdf
001_{order product summary}_{actual amount}/
002_{order product summary}_{actual amount}/
...
```

- Index starts at three-digit `001`; values above 999 are not truncated.
- Folder `name` joins non-empty matched-order product names in match/order-item order with `、`. Fall back to joined order numbers and then `订单`. Do not use an LLM in this version.
- Sanitize all archive path components for Linux and Windows, trim trailing dots/spaces, provide stable fallbacks, and cap the product portion at 80 characters.
- Each member folder contains the independent invoice preview as `发票_{number or id}.{pdf|png}` and all matched managed order images as `订单_01_{number or id}.{original extension}` in match order.
- Do not include the original invoice upload because it may contain other invoices.

The single `发票明细` worksheet contains, in order:

```text
序号, 发票类型, 发票代码, 发票号码, 开票日期,
购买方名称, 购买方税号, 销售方名称, 销售方税号,
不含税金额, 税额, 发票金额, 订单金额, 实际金额,
币种, 确认订单号
```

- One row per invoice in project order.
- Multiple order numbers join with `、` in match order.
- Money cells are numeric yuan with two-decimal formatting. Freeze and bold the header, enable filtering, and set practical column widths.

The combined PDF appends each project member in project order:

1. One page for each matched order record, preserving match order.
2. Every page of the independent invoice preview.

- Convert each order PNG/JPEG to one A4 portrait/landscape PDF page, preserving aspect ratio and fitting without cropping.
- Convert a PNG invoice preview to one page; append every page when the preview is PDF.
- Use a maintained image-to-PDF dependency plus the existing `lopdf` for object-safe merging.

Generate into same-directory temporary output. Complete the archive explicitly, then replace the destination. On failure remove new temporary artifacts and preserve an existing archive; on confirmed overwrite use a recoverable backup/restore replacement sequence compatible with Windows.

## Frontend Experience

- Add a Project navigation item and page consistent with the existing operational desktop layout.
- Project list supports search, create, rename, delete, status/progress, current valid actual amount, and opening details.
- Non-empty project deletion uses a destructive confirmation explaining that only membership is removed.
- Project detail is a table showing sequence, invoice identity, seller, order/product summary, confirmation state, all three amounts, and blocking reason.
- Use icon buttons with tooltips for move up/down. Disable unavailable directions.
- Export is visibly disabled for known invalid projects with its reason, but backend remains authoritative.
- Invoice ledger adds project membership display, an all/unassigned/specific-project filter, and selected-row actions to assign or remove membership.
- Assignment lists existing projects. Cross-project selection shows source projects and requires a second explicit confirmation before atomic reassignment.
- Refresh both affected workspaces after project or membership mutation.

## Out Of Scope

- LLM-generated folder names.
- Export history, saved ZIP snapshots, project locking, or project archival.
- Original invoice upload files and complete raw OCR/line-item expansion in Excel.
- Any change to matching rules, order global uniqueness, OCR/import queues, or existing matched edit/delete guards.

## Acceptance Criteria

- Fresh, previous-version upgrade, and interrupted-migration recovery tests pass; existing business data survives migration.
- Project name uniqueness and one-project-per-invoice constraints are enforced by SQLite.
- CRUD, atomic assignment/reassignment, removal/position compaction, and stale reorder validation are covered by repository tests.
- Amount tests cover invoice less/equal/greater than order sum, custom mismatch, and multiple orders.
- Empty, unconfirmed, missing/negative money, non-CNY, missing preview, and missing order image projects are rejected with specific reasons.
- A real archive test reopens the ZIP and verifies fixed filename, root entries, folder contents/order, safe names, XLSX columns/numeric values, and combined PDF page ordering/count.
- Failed generation leaves no partial target; failed overwrite preserves the old target.
- Frontend tests cover project CRUD, ordering, export state/directory/overwrite flows, invoice membership/filtering, and reassignment confirmation.
- `bun run lint`, `bun run build`, `bun run test`, Rust formatting check, Clippy, and Rust tests pass.
- Relevant Trellis backend/frontend contracts and `AGENTS.md` are updated with the durable project/export conventions.
