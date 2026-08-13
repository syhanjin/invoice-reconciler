<!-- TRELLIS:START -->
# Trellis Instructions

These instructions are for AI assistants working in this project.

This project is managed by Trellis. The working knowledge you need lives under `.trellis/`:

- `.trellis/workflow.md` — development phases, when to create tasks, skill routing
- `.trellis/spec/` — package- and layer-scoped coding guidelines (read before writing code in a given layer)
- `.trellis/workspace/` — per-developer journals and session traces
- `.trellis/tasks/` — active and archived tasks (PRDs, research, jsonl context)

If a Trellis command is available on your platform (e.g. `/trellis:finish-work`, `/trellis:continue`), prefer it over manual steps. Not every platform exposes every command.

If you're using Codex or another agent-capable tool, additional project-scoped helpers may live in:
- `.agents/skills/` — reusable Trellis skills
- `.codex/agents/` — optional custom subagents

Managed by Trellis. Edits outside this block are preserved; edits inside may be overwritten by a future `trellis update`.

<!-- TRELLIS:END -->

## Project Technology Conventions

- The desktop application uses Tauri v2 with a Rust backend.
- The Rust persistence layer uses SeaORM and `sea-orm-migration` with SQLite.
  Do not introduce SQLx as a separate application-facing persistence layer.
- The frontend uses React, TypeScript, shadcn/ui, Jotai, Tailwind CSS v4, and Bun.
- Use Bun for dependency management and frontend scripts. Do not introduce npm,
  pnpm, or Yarn lockfiles.
- The repository and internal engineering name is `invoice-reconciler`; the
  user-facing product name remains `票据助手`. The Tauri bundle identifier is
  `moe.syhanjin.invoice-reconciler`, and the SQLite database file is
  `db.sqlite3`.
- GitHub releases are created only from pushed `v*.*.*` tags. Preserve the
  Linux/Windows x86_64 matrix and publish both the native installer and the
  standalone executable for each platform.
- Pushes to `main` prebuild the same Linux/Windows release targets used by tag
  publishing. Keep Cargo package, full target-directory, and sccache layers
  aligned between build-check and publish workflows; only successful `main`
  builds may save shared caches, and retain the two newest target/sccache
  snapshots per platform.
- Initialize and add shadcn/ui components through `bunx shadcn@latest`; do not
  hand-copy shadcn component implementations.
- Prefer well-maintained existing dependencies over custom implementations on
  both the frontend and backend when they meet the requirement without adding
  disproportionate complexity.
- Linux launches default `WEBKIT_DISABLE_DMABUF_RENDERER` to `1` in the Rust
  executable entry point before Tauri starts. Preserve an explicitly supplied
  launch-environment value.
- Windows release executables use the GUI subsystem so launching the packaged
  application does not open a console window. Keep the CI PE-subsystem check;
  debug builds retain their console for diagnostics.
- Runtime settings live in a versioned application-data `settings.toml`, not
  SQLite. Keep the public settings response credential-free, preserve atomic
  replacement, and enforce `0600` on Unix or a current-user ACL on Windows.
  Ollama/OpenAI-compatible provider, endpoint, model, and API key are one global
  LLM configuration shared by order recognition and confirmed-match summaries;
  order recognition owns only its concurrency setting.
- Invoice OCR uses Tencent Cloud `RecognizeGeneralInvoice` through the dedicated
  invoice queue. Preserve local preflight/confirmation, the global 5 QPS limit,
  durable raw-response auditing, and the separate `invoice-task-updated` event.
- Order OCR uses one durable import pool. Ollama and generic OpenAI-compatible
  providers consume it through the dynamic realtime queue. Do not reintroduce
  Aliyun File/Batch submission, remote supervision, or order-side batch UI/API.
  Preserve restart recovery, atomic completion, immutable completion logs, and
  historical provider/model audit text for orders previously recognized by Aliyun.
- Order/invoice matching computes ephemeral candidates and persists only explicit
  confirmations. Preserve exact-fen/CNY rules, missing-date comparison semantics,
  global order uniqueness, atomic batch confirmation, and matched edit/delete guards.
  Ledger match state is a shared global projection: `confirmed` is durable,
  `matched` is one conflict-free automatic candidate, and all other unconfirmed
  records are `unmatched`; never derive it from one paged result.
  Suspected repurchases group orders by amount/normalized shop and invoices by
  amount/normalized seller/subtype, then pair equal-count groups by complete dates.
  Repeated dates are allowed and use record IDs only as stable same-day tie-breakers;
  missing dates retain a `疑似回购冲突`. Custom selection omits confirmed orders.
  Every confirmed relationship owns a durable asynchronous product-summary state.
  Generate it from all invoice line items and seller plus all matched-order product
  items and shops; summarize actual goods without requiring or inferring purpose.
  Preserve single-concurrency processing, startup backfill/recovery, input-hash stale
  response guards, explicit failed retry, provider/model audit, and deterministic
  product/order fallback names. Summary failure must never block matching or export.
- Projects own ordered invoice membership only; one invoice belongs to at most one
  project. Preserve transactional reassignment, position compaction after membership
  or invoice deletion, and project deletion without deleting invoices, matches, files,
  or orders. Archiving preserves the complete durable project graph and export, rejects
  name/order/membership mutations, and hides associated invoices and confirmed orders
  from order, invoice, and matching workspaces until unarchive.
- Project export is regenerated from current durable state and is allowed only when
  every member has a valid confirmed match, CNY/non-negative amounts, and readable
  independent invoice/order files. Keep money in integer fen, compute actual amount as
  `min(invoice total, confirmed order sum)`, preserve project order across XLSX/folders/PDF,
  use the backend match `displayName` for the XLSX order-summary column and material
  folder names, and use temporary generation plus recoverable overwrite for the final ZIP. Prefer an
  original invoice source only when it is proven to contain exactly that invoice; otherwise
  preserve extracted PDF pages or use a same-page cut/crop. Never rasterize PDF materials,
  downsample exported images, or use lossy image re-encoding. Preserve independent image
  bytes in the ZIP and use lossless image streams in the combined PDF.
- Deleting an imported order must preserve its immutable completion log while
  removing the order-owned managed screenshot to release the hash for reimport.
  Deleting an invoice records a source-hash plus
  subtype/number recovery marker so the same source can be reimported to restore
  only deleted invoices; source-less audit tasks must not permanently claim a
  file hash.
- Run SeaORM migrations before startup recovery queries. SQLite migration DDL
  must be safely repeatable, and schema changes require fresh-file, prior-version
  upgrade, and interrupted-migration recovery tests.

## Trellis Workflow Overrides

- Skip Trellis's interactive commit-confirmation step for this project.
- Create scoped commits automatically at appropriate implementation milestones.
- Never include unrelated or unrecognized worktree changes in those commits.
- After the task quality gate and required spec updates pass, run the Trellis
  finish-work flow automatically.
