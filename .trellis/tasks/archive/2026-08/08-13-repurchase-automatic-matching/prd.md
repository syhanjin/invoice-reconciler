# Optimize Repurchase Automatic Matching

## Goal

Resolve the ambiguous candidate graph created by repeated purchases whose business fields are identical except for order/invoice dates and unique identifiers. Pair the repeated orders and invoices chronologically when that ordering is reliable, otherwise retain a conflict with a repurchase-specific explanation. Hide already confirmed orders from the custom combination picker at the query boundary.

## Requirements

- Detect suspected repurchase groups only among unconfirmed, otherwise eligible records.
- Group orders by exact fen amount and normalized shop name. Ignore database ID, external order number, date, and product summary.
- Group invoices by exact fen amount, normalized seller name, and subtype. Ignore database ID, invoice number, and date.
- Apply special handling only when at least two orders and two invoices form one isolated homogeneous candidate relationship and every generated candidate is a single order.
- When counts match and both sides have complete, unique natural dates, sort ascending and expose only the same-index order as each invoice's automatic suggestion.
- Preserve all existing automatic rules, including CNY, exact-fen equality, date eligibility, global order uniqueness, candidate limits, and explicit confirmation.
- When counts differ, dates are missing/duplicated, or chronological pairing violates the existing date rule, keep status `conflict` and return a reason beginning with `疑似回购冲突`.
- Do not add a new top-level status, filter, summary field, badge, schema migration, or automatic persistence.
- Exclude occupied orders from `list_matchable_orders` before count, search, sorting, and pagination. Continue returning missing/negative-money orders as disabled.

## Acceptance Criteria

- Equal two-sided repurchase groups produce deterministic, non-overlapping chronological suggestions that pass single and batch confirmation revalidation.
- Unequal counts and ambiguous dates remain conflicts with the requested dedicated reason.
- Different amount, normalized shop, normalized seller, or invoice subtype prevents grouping; product summary differences do not.
- Occupied orders never appear in the custom picker and never contribute to its total.
- Existing ordinary one-to-one, many-to-one, overflow, custom mismatch, occupancy, and rollback behavior remains green.
- Rust tests, Clippy, frontend lint/tests/build, and relevant contracts pass/update.

## Out Of Scope

- Persisting automatic candidates or automatically confirming them.
- Comparing product summaries.
- Using IDs, creation time, or import order to break missing-date or same-date ties.
- Adding matching schema or settings.

## Technical Notes

- Primary implementation: `src-tauri/src/matching.rs`.
- Existing reason rendering in `src/components/matching-page.tsx` requires no new API field.
- Relevant contracts: `.trellis/spec/backend/order-invoice-matching-contract.md` and `.trellis/spec/frontend/matching-workspace-contract.md`.
