# Bill

A local-first Tauri desktop application for importing Taobao order screenshots,
recognizing structured order data with Ollama, and storing editable orders in
SQLite through SeaORM.

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
directory. The SQLite database is `bill.sqlite3`; managed screenshots are named
`images/<order-uuid>.<extension>`.
