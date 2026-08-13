# Remove Aliyun Bailian Batch API

## Goal

Remove every executable and user-facing part of the Aliyun Bailian File/Batch order-recognition provider while preserving the unified order task pool, Ollama and generic OpenAI-compatible realtime recognition, invoice-local batches, legacy order-import compatibility tables, existing orders, matches, and immutable completion audit records.

## Requirements

- Delete the Aliyun File/Batch HTTP client, remote batch manager, submission/cancel commands, supervision, polling, recovery, reconciliation, and provider-specific errors.
- Restrict order-recognition providers to `ollama` and `openai`; both use the realtime queue.
- Remove Aliyun settings, secrets, thinking configuration, public/request fields, frontend form controls, and service-check behavior.
- Upgrade `settings.toml` to a new version. Version 2 files must discard the Aliyun endpoint and key; `provider = "aliyun_batch"` must migrate to `ollama`. Preserve atomic replacement and platform permission hardening.
- Remove remote-batch fields and statuses from the order task-pool public contract, database model, frontend types, UI, API facade, and update events.
- Add a repeatable SQLite migration that rebuilds the order pool and immutable log schemas without batch fields and drops `order_recognition_batches`.
- During database upgrade, convert `batch_submitting` and `batch_locked` tasks to `failed`, clear their batch ownership, and provide a stable actionable error indicating that Batch support was removed and the task can be retried with a realtime provider.
- Preserve completed orders and completion logs. Historical `provider = "aliyun_batch"` and model text remain as audit data and render read-only as `阿里云百炼（历史）`; remove only the batch short number.
- Keep invoice batches and legacy `import_batches` / `import_tasks` behavior intact.
- Update `AGENTS.md` and executable Trellis backend/frontend specs so future work reflects the two realtime order providers and the migration/history contract.

## Acceptance Criteria

- [x] No source module, command, API method, settings field, UI control, or runtime recovery path can submit, poll, cancel, or resume an Aliyun batch.
- [x] `OrderRecognitionProvider` and its TypeScript equivalent contain only Ollama and OpenAI.
- [x] A fresh database reaches a final schema without `order_recognition_batches`, `recognition_batch_id`, Batch-only task statuses, or `batch_short_number`.
- [x] A version-5 database upgrades without losing normal pool tasks, logs, orders, managed-image metadata, or matches.
- [x] In-flight batch tasks become failed and manually retryable, with no automatic remote reconciliation or automatic retry during migration.
- [x] Version-1 and version-2 settings upgrade to the new settings schema; an Aliyun-selected version-2 file selects Ollama and no longer stores the Aliyun key.
- [x] Existing Aliyun completion logs keep their provider/model audit values and render as historical records.
- [x] Frontend lint, build, and tests pass; Rust formatting, clippy, and tests pass.
- [x] No invoice batch or legacy import-batch behavior regresses.

## Out Of Scope

- Deleting orders or logs previously produced by Aliyun.
- Contacting Aliyun to cancel or reconcile remote jobs during or after upgrade.
- Adding a dedicated Aliyun realtime provider. Users may configure a compatible service through the generic OpenAI provider.
- Removing invoice batches or the legacy order-import compatibility tables.

## Technical Notes

- Primary affected contracts are `.trellis/spec/backend/order-import-contract.md`, `.trellis/spec/backend/database-guidelines.md`, and `.trellis/spec/frontend/application-contract.md`.
- Migration ordering must remain before startup recovery queries.
- SQLite table rebuilds must tolerate an interrupted prior run and preserve foreign-key integrity.
- Existing unrelated untracked Trellis bootstrap files must not be included in functional commits unless directly updated for this task.
