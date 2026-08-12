# Fix reimport after deletion

## Goal

Allow an order screenshot or invoice source file to be imported again after its
corresponding business record is deleted. For a source that produced multiple
invoices, deleting any subset and importing the source again must restore only
the deleted invoices without duplicating invoices that still exist.

## Requirements

- Deleting an order must release the managed-image SHA-256 deduplication claim.
- Invoice duplicate audit tasks without a managed source must not permanently
  claim a source SHA-256.
- Deleting an invoice must durably record its source hash and normalized
  subtype/invoice-number recovery identity in the same database transaction as
  the invoice deletion.
- Any deletion marker for an invoice source permits that source to pass local
  preflight and the create-time race check.
- A recovery import must run through the existing paid-OCR confirmation flow.
- Recovery persistence must skip subtype/number identities that still exist, restore only
  missing identities with matching deletion markers, and consume markers for
  identities that are present after recovery.
- OCR results that do not match a deletion marker must not add new ledger rows
  during recovery.
- Once no deletion markers remain, the source must be treated as a duplicate
  again.
- Preserve file trash/restore compensation and durable raw-response auditing.
- Do not change public Tauri commands or frontend payloads.

## Acceptance Criteria

- An imported order can be deleted and the same screenshot is no longer
  skipped on its next import.
- A single invoice can be deleted and restored by reimporting the same source.
- If a source produced multiple invoices, deleting one and reimporting restores
  only that invoice; the identities and count return to the pre-delete set.
- Duplicate/failed invoice audit rows with no managed source do not block a
  valid recovery import.
- A failed invoice delete restores files and writes no deletion marker.
- Fresh, prior-version, and interrupted migrations are covered.
- Rust format, lint, tests, frontend lint/tests/build all pass.

## Assumptions

- Supported invoices have a stable invoice number; recovery matching uses the
  normalized subtype and invoice number without changing ordinary hard-duplicate rules.
- Reimporting an invoice source may call Tencent OCR again and therefore keeps
  the existing confirmation and cost warning.
- Historical task/audit rows remain available; the fix changes whether they
  participate in active deduplication.

## Out of Scope

- Frontend workflow or IPC shape changes.
- Restoring an invoice without rerunning OCR.
- Bulk cleanup of historical audit rows.

## Technical Notes

- Relevant contracts: `.trellis/spec/backend/order-import-contract.md`,
  `.trellis/spec/backend/invoice-import-contract.md`, and
  `.trellis/spec/backend/database-guidelines.md`.
- Primary implementation areas are the SeaORM migration/repository layer,
  invoice queue/review persistence paths, and deletion command tests.
