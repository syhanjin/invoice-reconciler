# 修复项目批量移除发票

## Goal

Restore the ability to remove one or more invoices from an active project in
the project detail workspace. Removing membership must preserve invoices,
orders, confirmed matches, summaries, and managed files.

## Requirements

- Active project detail shows a checkbox for every invoice member and a
  select-all checkbox with an indeterminate state.
- Selecting one or more members exposes one bulk "移出项目" action in the
  project workspace.
- Removal requires confirmation and sends all selected invoice IDs through
  the existing `remove_invoices_from_projects` command.
- A successful removal clears selection, reloads the current project detail
  and project list, refreshes the other workspaces, and shows one success
  toast containing the removed count.
- A failed removal keeps the selection and confirmation dialog available for
  retry and displays the backend error.
- Selection is cleared or reconciled whenever the selected project, project
  view, or authoritative detail membership changes.
- Archived projects remain read-only and expose no member selection or removal
  action. The backend remains authoritative for rejecting archived mutations.

## Acceptance Criteria

- [x] One or multiple project members can be selected and removed in one
  confirmed operation.
- [x] Canceling confirmation performs no command.
- [x] The command receives the complete selected invoice ID list.
- [x] Successful removal reloads project/list projections and invalidates the
  shared order, invoice, matching, and project projections.
- [x] Failed removal does not discard the user's selection.
- [x] Select-all and partial-selection states are correct.
- [x] Switching project or archived view does not retain stale selected IDs.
- [x] Archived project detail contains no removal controls.
- [x] Remaining project member positions are compact after bulk removal, while
  invoice and match records remain present.

## Definition of Done

- Frontend component and Rust repository regression tests cover the behavior.
- Frontend lint, tests, and production build pass.
- Rust formatting, Clippy, and tests pass.
- The project workspace contract reflects the restored project-detail removal
  workflow.
- Only task-owned files are committed; unrelated Trellis bootstrap files are
  excluded.

## Technical Approach

Add a typed frontend wrapper for the already registered backend command. Extend
the project member table with controlled checkbox selection and a confirmation
dialog, then reload authoritative projections after success. Reuse the existing
transactional backend removal implementation and add repository coverage for
active bulk removal and position compaction.

## Decision (ADR-lite)

**Context**: The backend still supports transactional membership removal, but
the frontend entry point was intentionally removed during the project archival
work. The requested workflow belongs in the project area and must support bulk
operations.

**Decision**: Provide bulk removal only in active project detail, not in the
invoice ledger and not as an immediate per-row action.

**Consequences**: Project detail gains selection state and one confirmation
flow. Archived projects stay read-only, and the existing invoice-ledger rule
against removal/reassignment remains unchanged.

## Out of Scope

- Restoring invoice-ledger removal or cross-project reassignment.
- Removing or modifying invoices, orders, matches, summaries, or files.
- Changing the backend command shape, database schema, or archive semantics.
- Adding removal controls to archived projects.

## Technical Notes

- Frontend: `src/lib/api.ts`, `src/components/projects-page.tsx`, and its
  component tests.
- Backend coverage: `src-tauri/src/projects.rs`.
- Specifications: frontend project workspace plus backend project/database
  contracts and the shared cross-layer guide.
- Existing untracked Trellis bootstrap files predate this task and must not be
  included in the work commit.
