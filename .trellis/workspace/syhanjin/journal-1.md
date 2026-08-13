# Journal - syhanjin (Part 1)

> AI development session journal
> Started: 2026-08-12

---



## Session 1: Taobao order screenshot OCR desktop app

**Date**: 2026-08-12
**Task**: Taobao order screenshot OCR desktop app
**Branch**: `main`

### Summary

Built and verified the first Tauri phase: Taobao screenshot OCR through bounded Ollama requests, UUIDv7 managed images, SeaORM SQLite persistence, and a React orders/settings workspace.

### Main Changes

- Added Tauri v2 commands, SeaORM entities and migrations, managed image storage, and transactional edit/delete behavior.
- Added structured Taobao OCR through Ollama with application-wide queue ownership, configurable concurrency, retry, cancel, and restart recovery.
- Added the React orders workspace, batch progress, editable order sheet, settings, typed API boundary, and responsive desktop styling.
- Added executable backend/frontend Trellis contracts and project conventions in `AGENTS.md`.

### Git Commits

| Hash | Message |
|------|---------|
| `227aa5b` | (see git log) |

### Testing

- [OK] `bun run build`, `bun run lint`, and 4 Vitest assertions.
- [OK] Rust formatting, 10 Cargo tests, and Clippy with warnings denied.
- [OK] Tauri release build and Debian bundle generation.
- [OK] Desktop smoke checks at 1360x840 and the 920x640 minimum window size.

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 2: Default Linux WebKit DMA-BUF renderer setting

**Date**: 2026-08-12
**Task**: Default Linux WebKit DMA-BUF renderer setting
**Branch**: `main`

### Summary

Defaulted WEBKIT_DISABLE_DMABUF_RENDERER to 1 at the Linux executable entry point before Tauri starts, while preserving explicit launch-environment overrides.

### Main Changes

- Added the Linux-only environment default before Tauri/WebKit initialization.
- Preserved explicit launch-environment values and documented the runtime contract.
- Updated README and `AGENTS.md` so development and future maintenance use the same behavior.

### Git Commits

| Hash | Message |
|------|---------|
| `94e783c` | (see git log) |

### Testing

- [OK] Rust formatting and 10 Cargo tests.
- [OK] Clippy with warnings denied.
- [OK] Linux release executable compilation.

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 3: Persistent import task center

**Date**: 2026-08-12
**Task**: Persistent import task center
**Branch**: `main`

### Summary

Added a first-level import task workspace with persisted batch pagination, on-demand task details, post-import navigation, responsive scrolling, cross-layer contracts, and full backend/frontend coverage.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `219777e` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 4: Complete order table controls

**Date**: 2026-08-12
**Task**: Complete order table controls
**Branch**: `main`

### Summary

Added server-controlled order sorting and pagination, persisted table display preferences, column resizing and visibility controls, responsive density and reset actions, plus frontend and backend regression coverage.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `4d78ba6` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 5: Order Selection And Bulk Deletion

**Date**: 2026-08-12
**Task**: Order Selection And Bulk Deletion
**Branch**: `main`

### Summary

Added current-page order selection with checked and indeterminate states, confirmation-driven bulk deletion, structured per-order Tauri results, safe sequential file compensation, partial-failure recovery, tests, and synced cross-layer specs.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `ba5e291` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 6: Invoice import and ledger

**Date**: 2026-08-12
**Task**: Invoice import and ledger
**Branch**: `main`

### Summary

Implemented secure versioned settings, Tencent general-invoice OCR preflight and queue, robust SeaORM invoice migrations with restart recovery tests, invoice review and ledger workflows, PDF/image previews, and multi-select bulk deletion.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `c01c542` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 7: Fix reimport after deletion

**Date**: 2026-08-13
**Task**: Fix reimport after deletion
**Branch**: `main`

### Summary

Allow deleted orders and invoices to be reimported; recover only deleted invoices from multi-invoice sources with transactional recovery markers and regression coverage.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `ce7da1b` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 8: 订单与发票匹配

**Date**: 2026-08-13
**Task**: 订单与发票匹配
**Branch**: `main`

### Summary

实现订单记录与发票的自动候选、冲突处理、自定义组合、确认持久化和匹配工作台。

### Main Changes

- Added durable many-to-one invoice matching with globally unique order occupancy, repeatable SeaORM migration coverage, automatic/custom confirmation, unconfirm, and atomic batch rollback.
- Implemented exact-fen CNY candidate generation, missing-money exclusion, missing-date skip semantics, same-shop multi-order normalization, cross-invoice conflicts, and bounded candidate search.
- Added the invoice-driven diff-style matching workspace with status filters, shared scrolling, candidate choice, batch confirmation, detail sheets, and a cross-page custom order picker with explicit mismatch acceptance.
- Protected confirmed records from deletion and warned/unconfirmed transactionally when matching-critical fields change.
- Added matching limits to versioned settings.toml and documented the backend/frontend matching contracts plus AGENTS.md conventions.
- Verification: cargo test (60 passed), cargo fmt --check, cargo clippy --all-targets -- -D warnings, bun run lint, bun run test (32 passed), bun run build, git diff --check, and desktop/minimum-window browser inspection with mocked Tauri IPC data.


### Git Commits

| Hash | Message |
|------|---------|
| `8dada77` | (see git log) |
| `80c6660` | (see git log) |

### Testing

- [OK] `cargo test`: 60 passed
- [OK] `cargo fmt --check`
- [OK] `cargo clippy --all-targets -- -D warnings`
- [OK] `bun run lint`
- [OK] `bun run test`: 32 passed across 9 files
- [OK] `bun run build`
- [OK] Desktop and 920x640 browser layout inspection with mocked Tauri IPC data

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 9: Preserve order import history on delete

**Date**: 2026-08-13
**Task**: Preserve order import history on delete
**Branch**: `main`

### Summary

Changed order deletion to preserve immutable import tasks while clearing managed image paths, added rollback and reimport regressions, and updated backend contracts.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `710deb2` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 10: Unified order recognition pool and Aliyun batch recovery

**Date**: 2026-08-13
**Task**: Unified order recognition pool and Aliyun batch recovery
**Branch**: `main`

### Summary

Replaced order import batches with one durable pool and immutable completion logs; added Ollama, OpenAI-compatible, and Aliyun File/Batch recognition with concurrent submissions, restart recovery, idempotent reconciliation, and same-session cancel release; updated responsive task/settings UI, migrations, tests, and executable specs.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `b80255a` | (see git log) |
| `d3aaff1` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 11: Remove Aliyun Batch API

**Date**: 2026-08-13
**Task**: Remove Aliyun Batch API
**Branch**: `main`

### Summary

Removed Aliyun Bailian File/Batch order recognition across Rust and React, migrated legacy settings and database state safely, preserved historical provider/model audits, and updated regression coverage and contracts.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `9b819e9` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 12: Invoice Reconciler release workflow

**Date**: 2026-08-13
**Task**: Invoice Reconciler release workflow
**Branch**: `master`

### Summary

Renamed internal project identifiers to invoice-reconciler while preserving the 票据助手 UI, changed the database to db.sqlite3, and added tag-driven Linux/Windows installer plus standalone executable releases.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `35c9f65` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 13: Project management and export packages

**Date**: 2026-08-13
**Task**: Project management and export packages
**Branch**: `main`

### Summary

Added ordered project membership, strict export eligibility, XLSX/material/PDF ZIP generation, project and invoice membership workflows, regression coverage, and executable contracts.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `0b75b8d` | (see git log) |
| `30fd9c2` | (see git log) |
| `6fd7fac` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 14: Repurchase automatic matching

**Date**: 2026-08-13
**Task**: Repurchase automatic matching
**Branch**: `main`

### Summary

Chronologically resolved isolated repurchase candidate groups, added dedicated suspected-repurchase conflict reasons, and excluded confirmed orders from custom selection.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `40fb430` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 15: Release v0.1.2 and optimize CI caches

**Date**: 2026-08-13
**Task**: Release v0.1.2 and optimize CI caches
**Branch**: `main`

### Summary

Fixed the Windows release console window, added dual-platform main prebuilds with layered Rust caches, and published v0.1.2.

### Main Changes

- Added Linux/Windows release prebuilds on `main` with separate Cargo package,
  Rust target, and sccache snapshots.
- Retained the two newest target and sccache snapshots per platform and kept
  pull-request/tag jobs read-only for shared caches.
- Switched Windows release binaries to the GUI subsystem and added a PE-header
  regression check to both build-check and publish workflows.
- Bumped the application to `0.1.2`, pushed `main`, and pushed tag `v0.1.2`.
- Verified frontend lint/build/tests, Rust formatting/clippy/tests, and a local
  Linux release/DEB build.


### Git Commits

| Hash | Message |
|------|---------|
| `23ad456` | (see git log) |
| `9680442` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 16: Preserve project export material fidelity

**Date**: 2026-08-13
**Task**: Preserve project export material fidelity
**Branch**: `main`

### Summary

Preserved original order and eligible invoice files during project export, switched combined PDF image pages to lossless full-resolution encoding, added material-selection and byte/pixel regression coverage, and updated the export contract.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `3dd4a1b` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 17: Ledger status and project archiving

**Date**: 2026-08-13
**Task**: Ledger status and project archiving
**Branch**: `main`

### Summary

Added global three-state order/invoice ledger status and filters, transactional invoice create-and-assign, active/archived project workflows, archived-project visibility guards, migration coverage, UI tests, and executable contract updates.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `70e5291a9072fd6b721e9f70a4b0466c13b1da1d` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 18: Allow repeated repurchase dates

**Date**: 2026-08-13
**Task**: Allow repeated repurchase dates
**Branch**: `main`

### Summary

Allowed complete repeated order and invoice dates in suspected-repurchase matching with deterministic same-day ID tie-breaking, retained missing-date and late-pair conflicts, and added regression coverage for suggestions and confirmations.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `f395f99` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 19: 全局 LLM 与匹配商品摘要

**Date**: 2026-08-13
**Task**: 全局 LLM 与匹配商品摘要
**Branch**: `main`

### Summary

将订单识别模型提升为全局 LLM 配置；为确认组合增加可恢复的异步商品摘要、确定性降级、匹配/项目展示和 XLSX/ZIP 导出；补齐迁移、协议、并发、编辑失效与前端重试测试。

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `e95d90c` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete
