# Preserve order import history on delete

## Goal

Deleting an order must not delete or rewrite the import task that records the
original import outcome. Remove the order business data and managed screenshot,
but preserve the completed task and batch history while allowing the same image
to be imported again.

## Requirements

- Preserve the `import_tasks` row, task status, batch relationship, source file
  name, SHA-256, error/warning fields, retry count, and timestamps.
- Delete the `orders` row and let its product items cascade.
- Clear `import_tasks.managed_image_path` in the same transaction as the order
  deletion so the old task no longer participates in image-hash deduplication.
- Keep the existing filesystem trash/restore compensation. Database failure
  must restore the image and roll back both order deletion and path clearing.
- Preserve the current `matching_locked` behavior for matched orders.
- Keep batch counts and task status unchanged after order deletion.
- Do not change public commands, payloads, frontend types, or UI flows.

## Acceptance Criteria

- A deleted order and its items are absent, while its succeeded import task and
  completed batch remain readable with unchanged aggregate counts.
- The task's managed image path is null, the old file is removed, and the image
  hash is available for a new import.
- A database failure restores the image and preserves the order, task path, and
  hash claim.
- A matched order deletion changes neither database rows nor files.
- Rust and frontend quality gates pass.

## Assumptions

- Import history is immutable audit history; it does not gain an `order deleted`
  status or flag.
- The original screenshot is intentionally removed on order deletion.
- Reimport creates a new task, order ID, and managed image.

## Technical Notes

- No schema migration is required: `managed_image_path` is nullable and
  `orders` already belongs to `import_tasks`, not the reverse.
- Update the project agent instructions and backend order/database contracts to
  prohibit deleting import audit rows as a consequence of order deletion.
