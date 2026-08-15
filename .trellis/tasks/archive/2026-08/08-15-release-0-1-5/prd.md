# 发布 0.1.5

## Goal

Publish the already verified fixes on `main` as patch release `0.1.5` and
trigger the tag-driven GitHub release workflow.

## Requirements

- Change the application version from `0.1.4` to `0.1.5` in `package.json`,
  `src-tauri/Cargo.toml`, the root package entry in `src-tauri/Cargo.lock`, and
  `src-tauri/tauri.conf.json`.
- Do not change dependency versions or release workflow behavior.
- Run frontend lint/tests/build and Rust formatting, Clippy, and tests before
  publishing.
- Commit only the four version files with
  `chore(release): bump version to 0.1.5`.
- Tag the dedicated release commit so `v0.1.5` follows the existing release
  pattern, then complete Trellis archival/journal bookkeeping on `main`.
- Create annotated tag `v0.1.5`, push `main`, push `v0.1.5` to `origin`, and
  push the subsequent Trellis bookkeeping commits to `main`.
- Verify local and remote branch/tag object IDs after push.

## Acceptance Criteria

- [x] All four application version sources report `0.1.5`.
- [x] Quality checks pass without modifying unrelated files.
- [x] The dedicated release commit precedes task archive and journal commits.
- [x] `v0.1.5` resolves to the dedicated release commit.
- [x] `origin/main` and remote `refs/tags/v0.1.5` match their local objects at
  release publication time; final bookkeeping is pushed by finish-work.

## Out of Scope

- Changing application behavior, dependencies, release assets, or workflows.
- Committing the unrelated Trellis bootstrap files from
  `00-bootstrap-guidelines`.
- Creating or editing the GitHub Release manually; the pushed tag owns that.

## Technical Notes

- Previous release commit `87a90b8` establishes the four-file version pattern.
- `.github/workflows/publish.yml` triggers only for pushed `v*.*.*` tags.
