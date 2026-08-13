# Invoice Reconciler

Invoice Reconciler is the repository and engineering name for the user-facing
`票据助手` desktop application. It imports Taobao order screenshots, recognizes
structured order data, manages invoices, and reconciles invoices with orders.
Data is stored locally in SQLite through SeaORM.

## Prerequisites

- Bun 1.3 or newer
- Rust 1.77 or newer
- Tauri v2 Linux system dependencies
- Ollama running locally or at a reachable HTTP endpoint
- The default vision model: `qwen3-vl:4b`

```bash
ollama pull qwen3-vl:4b
ollama serve
```

The Ollama URL, model, and application-wide request concurrency can be changed
from the Settings page.

## Development

```bash
bun install
bun run tauri dev
```

On Linux, the executable defaults `WEBKIT_DISABLE_DMABUF_RENDERER` to `1`
before WebKit starts. A value explicitly provided by the launch environment is
preserved.

Add shadcn/ui components with the official CLI:

```bash
bunx shadcn@latest add <component>
```

## Quality Checks

```bash
bun run build
bun run lint
bun run test

cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Build the Linux package:

```bash
bun run tauri build --bundles deb
```

Application data is stored in the platform-specific Tauri application data
directory. The SQLite database is `db.sqlite3`; managed screenshots are named
`images/<order-uuid>.<extension>`.

## Releases

Pushing a tag matching `v*.*.*` builds Linux and Windows x86_64 releases. The
GitHub Release contains a Debian or NSIS installer and a standalone executable
for each platform.
