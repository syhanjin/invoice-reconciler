# Project And Export Contract

## 1. Scope / Trigger

This contract applies when changing projects, invoice membership, project ordering, project summaries, export eligibility, ZIP/XLSX/PDF generation, or invoice deletion that can cascade a project membership. The Rust backend is authoritative for membership constraints, money, file validation, and export output.

## 2. Signatures

```text
list_projects(query: { page?, pageSize?, search?, archived? }) -> ProjectsPage
get_project(request: { id }) -> ProjectDetail
create_project(request: { name }) -> ProjectDetail
create_project_and_assign_invoices(request: { name, invoiceIds }) -> ProjectDetail
rename_project(request: { id, name }) -> ProjectDetail
delete_project(request: { id }) -> ()
archive_project(request: { id }) -> ()
unarchive_project(request: { id }) -> ()
assign_invoices_to_project(request: { invoiceIds, projectId, acceptReassignment }) -> AssignInvoicesResult
remove_invoices_from_projects(request: { invoiceIds }) -> ()
reorder_project_invoices(request: { projectId, invoiceIds }) -> ProjectDetail
export_project(request: { id, outputDirectory, overwrite }) -> ExportProjectResult
```

```text
projects 1 -- * project_invoices * -- 1 invoices
```

`projects.name` is unique and `projects.archived_at` is nullable normalized local time. `project_invoices.invoice_id` is globally unique, and `(project_id, position)` is project-locally unique. Both foreign keys cascade only the membership row.

## 3. Contracts

- Project names are trimmed, non-empty, at most 100 characters, and unique. Deleting a project never deletes invoices, orders, matches, or managed files.
- Active and archived projects are separate list projections selected by `archived`. Archiving sets `archived_at` without changing membership, matches, order occupancy, files, names, positions, or export eligibility; unarchive clears it.
- Archived projects remain viewable, exportable, deletable, and restorable, but reject rename, membership assignment/removal/reassignment, and reorder. Both the assignment target and every membership source project must be active.
- Daily order, invoice, and matching workspaces exclude archived-project members and their associated confirmed orders. Unarchive makes the same durable records visible again without reconstruction.
- `create_project_and_assign_invoices` validates the name, every invoice ID, and that every invoice is unassigned, then creates the project and appends all members in one transaction. Any validation or membership failure rolls back the project too.
- An unconfirmed invoice may be assigned. Assignment appends in input order. Moving an invoice from another project first returns conflicts without writing; only `acceptReassignment: true` moves every requested member in one transaction.
- Reorder receives the complete current member set exactly once. Missing, extra, duplicate, or stale IDs reject the whole request. Removal, reassignment, and invoice-deletion cascades compact positions to `0..n-1` and touch affected projects before commit.
- Project order is the single ordering source for detail rows, XLSX rows, material folder indexes, and combined PDF groups.
- For each member, `invoiceAmountFen = invoices.total_fen`, `orderAmountFen = sum(confirmed orders.paid_amount_fen)`, and `actualAmountFen = min(invoiceAmountFen, orderAmountFen)`. Project actual amount is the sum of currently valid member actual amounts. Money remains integer fen until XLSX/UI/file-name presentation.
- Export requires a non-empty project. Every member must have a persisted confirmed match containing at least one order, CNY/non-negative invoice money, non-negative order money, a readable independent invoice preview, and readable matched-order managed images. Backend revalidates current SQLite and filesystem state before generating anything.
- The output name is `{safe project name}_{actual yuan to two decimals}.zip`. The ZIP root contains `发票明细.xlsx`, `组合材料.pdf`, and one `NNN_{product summary}_{actual amount}` folder per member.
- Each member folder contains one independent invoice material and every matched order image. Prefer the managed original invoice source only when its complete successful OCR response belongs to that one invoice, no source page contains multiple recognized items, and a PDF invoice spans the complete source PDF. Otherwise use the independent preview: keep partial PDFs as extracted PDF pages, and use an image only when same-page invoices require an OCR cut or local crop. Never export a source that may contain another invoice.
- Preserve independent order and invoice image bytes in the ZIP and store image entries without ZIP recompression. The XLSX sheet has one ordered row per member and numeric yuan cells for invoice, order, and actual amounts.
- The combined PDF appends every matched order as one fitted A4 image page, then every page of the selected independent invoice material. Image pages retain their original pixel dimensions, are never downsampled, and use only lossless PDF image encoding. Use `lopdf` object merging for existing PDF pages; do not rasterize, parse, or redraw them.
- Generate to a same-directory temporary file. A new same-name target that appears during generation still requires overwrite acceptance. Confirmed replacement uses backup/restore; any generation or replacement failure cleans temporary output and preserves the prior archive whenever restoration succeeds.

## 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Blank/overlong/duplicate project name | `invalid_input`; write nothing |
| Create-and-assign contains a missing or already assigned invoice | `invalid_input`; create neither project nor memberships |
| Invoice is already in another project, acceptance false | Return `assigned: false` plus conflicts; write nothing |
| Assignment/reorder contains missing or stale members | `invalid_input`; roll back every membership write |
| Rename/reorder/membership mutation touches an archived source or target | `invalid_input`; preserve project and memberships |
| Empty project or any unconfirmed member | `exportable: false`; `export_project` rejects with a concrete reason |
| Missing/negative money, non-CNY invoice, empty match, or missing file | Block the complete export; never emit a partial package |
| Output directory missing/not a directory | `invalid_input` before generation |
| Same-name ZIP exists, overwrite false | `destination_exists`; preserve the target |
| Generation/replacement fails | Remove new temporary artifacts and preserve/restore the old target |

## 5. Good/Base/Bad Cases

- Good: a two-order confirmed invoice uses the order sum, chooses the lower invoice/order amount, stores both order images byte-for-byte, and emits two lossless order pages before all invoice pages.
- Good: a complete single-invoice PDF is merged from the managed source; a partial multi-invoice PDF uses extracted PDF pages; same-page invoices use their independent cut or cropped images.
- Good: deleting the first invoice from a project compacts the second member from position `1` to `0`, so a later append cannot violate the unique position constraint.
- Good: archiving a project hides its ledger/matching records, keeps export available, and unarchiving restores visibility with the same membership and confirmed relationships.
- Base: an invoice can be organized before matching; the project remains editable but export stays disabled until confirmation and file checks pass.
- Bad: clearing membership on archive, allowing an archived source project to be reassigned through a lower-level command, trusting frontend `exportable`, storing yuan as float, exporting an unproven multi-invoice source, rasterizing a PDF, downsampling or JPEG-reencoding an image, silently overwriting a ZIP, or leaving position gaps after a cascade violates this contract.

## 6. Tests Required

- Migration tests: fresh database, seeded m0007 upgrade, interrupted m0008 recovery, nullable archive timestamp, one-project-per-invoice uniqueness, project-position uniqueness, and both cascade directions.
- Repository tests: unique names, atomic create-and-assign rollback, assignment preview/acceptance, transactional reassignment, archived source/target mutation rejection, removal and invoice-deletion compaction, complete-set reorder rejection, archive/unarchive list visibility, and the less/equal/greater actual-amount cases.
- Eligibility tests: empty project, unconfirmed member, missing/negative invoice or order money, non-CNY, empty match, missing invoice preview, and missing order image.
- Archive tests must reopen the ZIP/XLSX/PDF and assert fixed root names, safe folder/file names, all 16 XLSX headers, numeric money cells, byte-for-byte independent image preservation, order-first page sequence/count, original image dimensions with lossless PDF filters, and multi-page invoice retention without rasterization.
- Material-selection tests must cover complete single-invoice images/PDFs, partial PDF ranges, same-page multi-invoice sources, malformed or partial OCR audits, and missing source fallback.
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

Wrong:

```rust
document.save(&PdfSaveOptions::default(), &mut warnings);
// The default image optimization may downsize and JPEG-reencode color images.
```

Correct:

```rust
let options = PdfSaveOptions {
    image_optimization: Some(ImageOptimizationOptions {
        max_image_size: None,
        auto_optimize: Some(false),
        format: Some(ImageCompression::Flate),
        ..Default::default()
    }),
    ..Default::default()
};
document.save(&options, &mut warnings);
```

Wrong: remove project memberships when setting `archived_at`, or allow archived members to be moved through a direct command.

Correct: archive only the project row, preserve its complete durable graph, and enforce active-project checks at every membership/name/order mutation boundary.
