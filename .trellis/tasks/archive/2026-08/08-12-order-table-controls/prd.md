# Complete Order Table Controls

## Goal

Upgrade the orders workspace from a mostly read-only table into a configurable,
persisted operational data table while keeping all data access server-paged and
server-sorted.

## Requirements

- Keep the orders query controlled by the page with `page`, `pageSize`, `search`,
  `duplicateOnly`, `sortBy`, and `sortDirection`.
- Reset to page one after search, duplicate filtering, sorting, or page-size
  changes. Preserve the active query after refreshes and order mutations.
- Support single-column server sorting for shop name, paid amount, ordered time,
  and created time. Cycle sortable headers through ascending, descending, and
  the default created-time-descending sort.
- Add a visible created-time column. Keep order number, product, and source
  columns non-sortable.
- Allow optional columns to be shown or hidden while keeping the order number
  column always visible.
- Support constrained column resizing without row activation and comfortable or
  compact row density.
- Add a reset action for all persisted table preferences.
- Provide page sizes 20, 50, and 100; first, previous, next, and last navigation;
  validated page-number entry; current result range; total count; and current
  page / total pages.
- Correct an invalid current page when result counts shrink. Empty results show
  page 1 of 1.
- Persist page size, sort, column visibility, column sizing, and density locally
  through a versioned Jotai storage atom with runtime normalization. Do not
  persist search, duplicate filtering, current page, or an open order.
- Keep current successful rows visible when a reload fails, surface the error via
  Sonner, and allow later interactions to retry.
- Do not add column reordering, pinning, row selection, or bulk business actions.

## Public Interfaces

- Preserve the Tauri command and Rust `OrdersQuery` wire shape.
- Continue using the existing sort values: `created_at`, `ordered_at`,
  `paid_amount`, and `shop_name`, with `asc` or `desc` direction.
- Make `OrdersTable` controlled for pagination, sorting, and table preferences;
  `OrdersPage` remains the owner of remote query execution.
- Add frontend-only types for table preferences and density. No database
  migration or backend settings persistence is required.

## Acceptance Criteria

- [ ] Clicking each sortable header issues the matching server query and resets
      the page to one.
- [ ] Search, duplicate-only filtering, and page-size changes retain all other
      query values and reset the page.
- [ ] Page-size selection, all four navigation buttons, valid jump input, and
      invalid or out-of-range jump handling behave predictably.
- [ ] A page beyond the new last page is corrected automatically; empty results
      remain at page 1 of 1.
- [ ] Column visibility, minimum-visible-column behavior, resizing, density, and
      reset-to-default are usable at supported window sizes.
- [ ] Table preferences survive remounts, while malformed or obsolete stored
      values normalize to defaults.
- [ ] Resizing and header controls do not open an order; clicking a data row does.
- [ ] Backend tests cover all sort fields in both directions, stable tie-breaking,
      and combinations with search, duplicate filtering, and pagination.
- [ ] `bun run build`, `bun run lint`, `bun run test`, and relevant Rust tests pass.

## Out of Scope

- Import-task history controls.
- Multi-row selection and bulk actions.
- Cross-device preference synchronization.
- Database schema changes.

## Technical Notes

- Use existing TanStack Table, Jotai, shadcn/ui, and Lucide React dependencies.
- The existing backend already implements the four required server sort fields.
- The frontend application contract must be updated with the new table behavior.
- Preserve unrelated or unrecognized worktree changes and commit only task-owned
  files plus task bookkeeping.
