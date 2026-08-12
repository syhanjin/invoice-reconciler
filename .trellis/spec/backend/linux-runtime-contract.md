# Linux Runtime Contract

## Scenario: WebKit DMA-BUF Renderer Default

### 1. Scope / Trigger

This contract applies when changing the native executable entry point, Linux packaging, or WebKit graphics environment. Linux launches need a compatibility default without modifying the user's shell or affecting other operating systems.

### 2. Signatures

The executable startup order is:

```text
main()
  -> apply Linux process-environment default
  -> bill_lib::run()
  -> Tauri/WebKit initialization
```

There is no Tauri command or database signature for this process-level setting.

### 3. Contracts

- Environment key: `WEBKIT_DISABLE_DMABUF_RENDERER`.
- Default value on Linux: `1`.
- An existing value, including an empty string or `0`, is explicit and must be preserved.
- The environment write occurs only in the Linux-compiled branch of `src-tauri/src/main.rs`.
- The write must finish before calling `bill_lib::run()` so WebKit and any spawned threads inherit the value.
- Development runs and packaged launches use the same executable entry point.

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Linux and key absent | Set the key to `1` before Tauri starts |
| Linux and key present | Preserve its exact value |
| Non-Linux target | Compile no environment-mutation branch |
| Proposed write after Tauri initialization | Reject the change because other threads may already exist |

### 5. Good/Base/Bad Cases

- Good: launching the packaged Linux executable with no graphics override makes WebKit see `WEBKIT_DISABLE_DMABUF_RENDERER=1`.
- Base: `WEBKIT_DISABLE_DMABUF_RENDERER=0 bun run tauri dev` keeps `0`, allowing a deliberate override.
- Bad: setting the variable only in a development script misses desktop-menu and packaged launches; setting it unconditionally removes user control.

### 6. Tests Required

- Run Rust formatting and Clippy with warnings denied.
- Run the Rust test suite to catch bootstrap compilation regressions.
- Compile the Linux release executable and assert `src/main.rs` is part of the binary target.
- When extracting startup environment logic into a function, add tests for absent and present values without mutating process environment concurrently.

### 7. Wrong vs Correct

#### Wrong

```rust
bill_lib::run();
std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
```

This runs too late and unconditionally overwrites a user choice.

#### Correct

```rust
#[cfg(target_os = "linux")]
if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
    // Safe only because the application has not started any threads yet.
    unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
}
bill_lib::run();
```
