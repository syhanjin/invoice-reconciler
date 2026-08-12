# Default Linux WebKit DMA-BUF Environment

## Goal

Make Linux desktop launches default to `WEBKIT_DISABLE_DMABUF_RENDERER=1` before Tauri initializes WebKit, improving renderer compatibility without requiring shell-specific launch commands.

## Requirements

- Apply only when compiling for Linux.
- Set the environment variable before `bill_lib::run()` starts Tauri or WebKit threads.
- Treat `1` as a default: preserve any value explicitly supplied by the user or launcher.
- Cover development runs and packaged application launches through the same executable entry point.
- Document the Linux runtime default in the README.

## Acceptance Criteria

- [x] A Linux launch with no existing value sees `WEBKIT_DISABLE_DMABUF_RENDERER=1` before Tauri starts.
- [x] An explicitly configured value is not overwritten.
- [x] Non-Linux builds do not execute or require this environment mutation.
- [x] Rust formatting, tests, Clippy, and a compile/build check pass.

## Out Of Scope

- Changing other WebKit or graphics environment variables.
- Modifying user shell profiles or system-wide environment configuration.
- Adding a settings-screen control for this low-level startup option.

## Technical Notes

- Rust edition 2024 marks process environment mutation as unsafe because concurrent mutation can race with other threads. The write must therefore be the first operation in `main`, with a safety comment explaining that Tauri has not started any threads.
- The executable entry point is `src-tauri/src/main.rs`.
