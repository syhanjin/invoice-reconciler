# Invoice Reconciler Rename And Publish CI

## Goal

Adopt `invoice-reconciler` as the repository and internal engineering name while
preserving the user-facing product name `票据助手`, then add a tag-driven GitHub
Actions workflow that publishes Linux and Windows installers and standalone
executables.

## Requirements

- Rename the npm package, Rust package/binary, Rust library crate, Tauri bundle
  identifier, internal preference key, logging target, and release assets to
  `invoice-reconciler` or the syntax-appropriate underscore form.
- Use the Tauri bundle identifier `moe.syhanjin.invoice-reconciler`.
- Preserve `票据助手` in the Tauri product name, window title, HTML title, and
  React application shell.
- Rename the SQLite file to `db.sqlite3`; no migration is required because the
  application has not been released.
- Publish only on pushed tags matching `v*.*.*`; do not expose a manual trigger.
- Build Linux x86_64 and Windows x86_64 in parallel without fail-fast.
- Publish both platform installers (`.deb`, NSIS `.exe`) and standalone
  executables with ASCII names containing project, platform, architecture, and
  tag.
- Generate GitHub Release notes and permit release asset writes through the
  workflow-scoped token.
- Update README and AGENTS.md so project naming and release conventions remain
  discoverable.

## Acceptance Criteria

- [x] Internal project identifiers use `invoice-reconciler`; visible UI remains
      `票据助手`.
- [x] The runtime database path is `db.sqlite3`.
- [x] A valid GitHub Actions workflow implements the agreed tag trigger, build
      matrix, packaging, and release behavior.
- [x] Frontend build, lint, and tests pass.
- [x] Rust format, clippy, and tests pass.
- [x] A local Linux Tauri build produces a Debian package and standalone binary.
- [x] Only task-owned changes are committed; unrelated bootstrap changes remain
      untouched.

## Out Of Scope

- Renaming the GitHub repository through GitHub settings.
- Migration from old database, settings, preference, or application-data paths.
- macOS, ARM, signing, notarization, or updater manifests.
- Automatic synchronization of source versions from the pushed tag.

## Technical Notes

- Reference workflow: `/home/syhanjin/workspace/robocon2026/master/26-r2-controller/.github/workflows/publish.yml`.
- The repository uses Tauri v2, Bun, React/TypeScript, and Rust.
- Existing untracked Trellis/bootstrap files predate this task and must not be
  included unless directly required by the task workflow.
