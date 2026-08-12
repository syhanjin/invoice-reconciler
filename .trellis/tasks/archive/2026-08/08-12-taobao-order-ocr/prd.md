# Taobao Order Screenshot OCR Desktop App

## Goal

Build the first phase of an invoice-matching desktop application: import one or more Taobao order screenshots, recognize one order per screenshot through a configurable local Ollama vision model, copy managed images into application storage, and persist editable normalized order data in SQLite.

## Product Requirements

- Use Tauri v2 with a Rust backend and a React/TypeScript frontend.
- Use Bun for frontend package management and scripts, Tailwind CSS v4, shadcn/ui components installed through `bunx shadcn@latest add`, Jotai for client state, TanStack Table, React Hook Form, Zod, Sonner, and Lucide React.
- Prefer maintained third-party dependencies over custom infrastructure on both frontend and backend.
- First screen is the operational orders workspace, with Orders and Settings navigation only.
- Import multiple supported image files through a native picker or drag/drop. The only platform option in v1 is Taobao.
- One screenshot represents exactly one order; an order may contain multiple products.
- Each non-duplicate import gets a UUIDv7. The task, order, and managed image share this UUID; the copied file is `images/<uuid>.<normalized-extension>`.
- Compute SHA-256 before OCR. An identical image is skipped without copying or OCR. Different images with the same non-empty order number are both retained and all records in that group are marked as possible duplicates.
- OCR success is immediately persisted and can be edited in an order detail sheet. Preserve the original model JSON as read-only audit data.
- Deleting an order also deletes its managed image, with compensation so a database failure does not orphan or lose the image.

## OCR And Queue Requirements

- Call Ollama directly from Rust at a configurable base URL; do not ship a Python sidecar.
- Defaults: `http://127.0.0.1:11434`, `qwen3-vl:4b`, and maximum concurrency `3`; concurrency is configurable from `1` through `8`.
- Check server and model availability, show actionable start/pull guidance, and never install Ollama or pull a model automatically.
- Submit `/api/chat` with a base64 image, `stream: false`, `num_ctx: 16384`, `temperature: 0`, the Taobao prompt, and a JSON Schema.
- Structured result fields are `order_number: string|null`, `product_names: string[]`, `paid_amount: number|null`, `ordered_at: string|null`, and `shop_name: string|null`.
- Use an application queue and Tokio semaphore. The count of sent, unfinished HTTP requests must never exceed the batch's captured concurrency setting; Ollama may queue already-sent requests internally.
- Support retrying one failed/interrupted task, retrying all failed tasks, and canceling tasks that have not been sent. Do not force-abort sent requests.
- On restart, convert processing tasks to interrupted. Do not resubmit them automatically.
- Store monetary values as integer fen after decimal parsing. Normalize recognized time to local `YYYY-MM-DD HH:mm`. Invalid optional values become null with a warning while valid fields are still saved.

## Data And Interface Requirements

- Rust owns SQLite through SeaORM and applies `sea-orm-migration` versioned migrations on startup. The frontend never executes SQL.
- Persist application settings, import batches, import tasks, orders, and ordered product items. Image hash is unique; external order number is not.
- Expose typed Tauri commands for bootstrap state, Ollama checks, settings, import batches, task cancellation/retry, batch lookup, paged order queries, order detail, update, and delete.
- Emit one typed `import-task-updated` event. Jotai merges live events, while SQLite remains the source of truth.
- Restrict image preview access to the managed application image directory.
- Orders query uses server-side pagination, search, status filtering, and sorting.

## User Experience

- Orders page contains import controls, search/filter controls, a compact order table, and an expandable batch progress panel with queued, processing, success, failed, canceled, skipped, and interrupted states.
- Selecting an order opens a sheet with its screenshot and editable order fields.
- Settings page edits Ollama URL, model, and concurrency and includes connection testing.
- Use shadcn controls and Lucide icons, restrained neutral styling, responsive stable dimensions, and no marketing/landing page.

## Acceptance Criteria

- [x] A successful screenshot import produces a UUIDv7 order, a UUID-named managed image, normalized order fields, ordered products, and raw OCR JSON.
- [x] Identical image content is skipped before OCR; same order number from different images is retained and marked possible duplicate.
- [x] Active Ollama HTTP requests never exceed configured concurrency and queued tasks can be canceled.
- [x] Failed and interrupted tasks can be retried without changing their UUID or image.
- [x] Order edits and duplicate recalculation are transactional; deletion removes both data and managed image with compensation.
- [x] Frontend provides the complete orders, batch, edit, delete, and settings workflows.
- [x] Rust tests, Clippy, formatting, frontend tests, lint/typecheck, and Linux Tauri build pass.
- [x] `AGENTS.md` documents the project stack, dependency-first policy, Bun/shadcn commands, and automatic commit/finish convention.

## Out Of Scope

- Invoice import or invoice-to-order matching.
- Platforms other than Taobao.
- Automatic model download or Ollama installation.
- Force-canceling already-sent Ollama requests.
- Windows or macOS packaging verification.

## Workflow Overrides

- The project skips Trellis's per-commit confirmation. Create appropriate scoped commits automatically.
- After quality checks and required spec updates, run finish-work automatically.
- Never include unrelated or unrecognized existing worktree changes in a feature commit.
