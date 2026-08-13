# Project And Export Contract

## 1. Scope / Trigger

This contract applies when changing projects, invoice membership, project ordering, project summaries, export eligibility, ZIP/XLSX/PDF generation, or invoice deletion that can cascade a project membership. The Rust backend is authoritative for membership constraints, money, file validation, and export output.

## 2. Signatures

```text
list_projects(query: { page?, pageSize?, search? }) -> ProjectsPage
get_project(request: { id }) -> ProjectDetail
create_project(request: { name }) -> ProjectDetail
rename_project(request: { id, name }) -> ProjectDetail
delete_project(request: { id }) -> ()
assign_invoices_to_project(request: { invoiceIds, projectId, acceptReassignment }) -> AssignInvoicesResult
remove_invoices_from_projects(request: { invoiceIds }) -> ()
reorder_project_invoices(request: { projectId, invoiceIds }) -> ProjectDetail
export_project(request: { id, outputDirectory, overwrite }) -> ExportProjectResult
```

```text
projects 1 -- * project_invoices * -- 1 invoices
```

`projects.name` is unique. `project_invoices.invoice_id` is globally unique, and `(project_id, position)` is project-locally unique. Both foreign keys cascade only the membership row.

## 3. Contracts

- Project names are trimmed, non-empty, at most 100 characters, and unique. Deleting a project never deletes invoices, orders, matches, or managed files.
- An unconfirmed invoice may be assigned. Assignment appends in input order. Moving an invoice from another project first returns conflicts without writing; only `acceptReassignment: true` moves every requested member in one transaction.
- Reorder receives the complete current member set exactly once. Missing, extra, duplicate, or stale IDs reject the whole request. Removal, reassignment, and invoice-deletion cascades compact positions to `0..n-1` and touch affected projects before commit.
- Project order is the single ordering source for detail rows, XLSX rows, material folder indexes, and combined PDF groups.
- For each member, `invoiceAmountFen = invoices.total_fen`, `orderAmountFen = sum(confirmed orders.paid_amount_fen)`, and `actualAmountFen = min(invoiceAmountFen, orderAmountFen)`. Project actual amount is the sum of currently valid member actual amounts. Money remains integer fen until XLSX/UI/file-name presentation.
- Export requires a non-empty project. Every member must have a persisted confirmed match containing at least one order, CNY/non-negative invoice money, non-negative order money, a readable independent invoice preview, and readable matched-order managed images. Backend revalidates current SQLite and filesystem state before generating anything.
- The output name is `{safe project name}_{actual yuan to two decimals}.zip`. The ZIP root contains `发票明细.xlsx`, `组合材料.pdf`, and one `NNN_{product summary}_{actual amount}` folder per member.
- Each member folder contains the independent invoice preview and every matched order image. It must not include the original uploaded invoice source because that source may contain other invoices.
- The XLSX sheet has one ordered row per member and numeric yuan cells for invoice, order, and actual amounts. The combined PDF appends every matched order as one fitted A4 image page, then every page of the independent invoice preview. Use `lopdf` object merging for existing PDF pages; do not parse and redraw them.
- Generate to a same-directory temporary file. A new same-name target that appears during generation still requires overwrite acceptance. Confirmed replacement uses backup/restore; any generation or replacement failure cleans temporary output and preserves the prior archive whenever restoration succeeds.

## 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Blank/overlong/duplicate project name | `invalid_input`; write nothing |
| Invoice is already in another project, acceptance false | Return `assigned: false` plus conflicts; write nothing |
| Assignment/reorder contains missing or stale members | `invalid_input`; roll back every membership write |
| Empty project or any unconfirmed member | `exportable: false`; `export_project` rejects with a concrete reason |
| Missing/negative money, non-CNY invoice, empty match, or missing file | Block the complete export; never emit a partial package |
| Output directory missing/not a directory | `invalid_input` before generation |
| Same-name ZIP exists, overwrite false | `destination_exists`; preserve the target |
| Generation/replacement fails | Remove new temporary artifacts and preserve/restore the old target |

## 5. Good/Base/Bad Cases

- Good: a two-order confirmed invoice uses the order sum, chooses the lower invoice/order amount, stores both order images, and emits two order pages before all invoice pages.
- Good: deleting the first invoice from a project compacts the second member from position `1` to `0`, so a later append cannot violate the unique position constraint.
- Base: an invoice can be organized before matching; the project remains editable but export stays disabled until confirmation and file checks pass.
- Bad: trusting frontend `exportable`, storing yuan as float, exporting the original multi-invoice upload, silently overwriting a ZIP, or leaving position gaps after a cascade violates this contract.

## 6. Tests Required

- Migration tests: fresh database, seeded m0006 upgrade, interrupted m0007 recovery, one-project-per-invoice uniqueness, project-position uniqueness, and both cascade directions.
- Repository tests: unique names, assignment preview/acceptance, transactional reassignment, removal and invoice-deletion compaction, complete-set reorder rejection, and the less/equal/greater actual-amount cases.
- Eligibility tests: empty project, unconfirmed member, missing/negative invoice or order money, non-CNY, empty match, missing invoice preview, and missing order image.
- Archive tests must reopen the ZIP/XLSX/PDF and assert fixed root names, safe folder/file names, all 16 XLSX headers, numeric money cells, original invoice-preview preservation, order-first page sequence/count, and multi-page invoice retention.
- Failure tests must prove an existing ZIP survives generation failure and temporary files are removed. Test `destination_exists` and confirmed overwrite behavior at the command/service boundary.
- Run Rust formatting, Clippy with warnings denied, and the complete Rust test suite.

## 7. Wrong vs Correct

Wrong:

```rust
let actual_yuan = invoice_yuan.min(order_yuan); // floating-point business money
std::fs::write(target, generate_zip()?);         // truncates the old target first
```

Correct:

```rust
let actual_fen = invoice_fen.min(order_fen);
generate_archive(&snapshot, &temporary)?;
// Install only after generation succeeds; backup and restore a confirmed overwrite.
```
