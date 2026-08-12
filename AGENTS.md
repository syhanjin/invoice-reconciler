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
- Initialize and add shadcn/ui components through `bunx shadcn@latest`; do not
  hand-copy shadcn component implementations.
- Prefer well-maintained existing dependencies over custom implementations on
  both the frontend and backend when they meet the requirement without adding
  disproportionate complexity.

## Trellis Workflow Overrides

- Skip Trellis's interactive commit-confirmation step for this project.
- Create scoped commits automatically at appropriate implementation milestones.
- Never include unrelated or unrecognized worktree changes in those commits.
- After the task quality gate and required spec updates pass, run the Trellis
  finish-work flow automatically.
