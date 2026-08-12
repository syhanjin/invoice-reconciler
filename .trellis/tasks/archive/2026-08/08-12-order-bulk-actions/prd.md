# Order Selection And Bulk Actions

## Goal

Extend the configurable orders table with row selection and safe deletion
workflows so users can remove multiple imported orders without opening each
detail sheet.

## What I Already Know

- The orders table is server-paged and already uses controlled TanStack Table
  state for sorting, pagination, visibility, sizing, and density.
- Single-order deletion already removes both the order/task database records and
  the managed screenshot. It moves the image to application trash first,
  restores it if the database transaction fails, and removes the trash file
  after success.
- The detail sheet already presents a destructive confirmation dialog and must
  retain its single-order deletion behavior.
- A frontend `Promise.all` over `api.deleteOrder` would produce unclear partial
  failure behavior and unnecessary concurrent filesystem/database mutations.

## Decisions

- Selection is scoped to rows on the currently loaded server page. It is cleared
  by page, page-size, search, duplicate-filter, or sort changes and pruned after
  refreshes when selected rows no longer exist.
- The only bulk action in this task is delete. Selection count and
  clear-selection are part of the table toolbar; editing and export are out of
  scope.
- Bulk deletion uses one typed Tauri command and returns structured per-order
  success/failure results rather than failing the entire response after a
  partial deletion.

## Requirements (Evolving)

- Add an always-visible selection column with a header checkbox for all rows on
  the current page and a checkbox for each row.
- Checkbox interaction must not open the order detail sheet.
- Show the selected count and a clear-selection action while selection is active.
- Provide a destructive bulk-delete action with one confirmation dialog that
  states the selected count and that managed screenshots are also deleted.
- Disable repeated submission while deletion is running.
- Reuse the existing file-compensation and duplicate-flag recalculation rules for
  every deleted order.
- Refresh the current server query after deletion, correct invalid pages, clear
  successfully deleted selections, and surface failures with actionable IDs or
  order numbers.
- Keep single-row detail selection and single-order deletion working.

## Acceptance Criteria (Evolving)

- [x] Header selection controls only the currently loaded page and reflects
      checked, unchecked, and indeterminate states.
- [x] Row checkbox and bulk toolbar interactions never open the detail sheet.
- [x] Canceling confirmation changes neither files nor database records.
- [x] Confirming deletion removes each successful order and its managed image,
      refreshes the table, and reports partial failures without hiding successes.
- [x] Selection remains consistent after refreshes, page correction, query
      changes, deletion failures, and empty results.
- [x] Frontend and Rust tests cover selection, confirmation, success, partial
      failure, duplicate recalculation, and filesystem compensation.

## Out Of Scope

- Cross-page or all-filtered-results selection.
- Bulk editing, export, tagging, or status changes.
- Undo after confirmed deletion.

## Technical Notes

- Continue using TanStack Table row selection and existing shadcn/ui controls.
- Add backend request/response types and an API wrapper rather than invoking
  single deletion concurrently from React.
- Do not add a database migration.
