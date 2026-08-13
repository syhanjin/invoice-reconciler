# Frontend Development Guidelines

> Executable conventions for the React application embedded in Tauri.

## Stack

Use React with TypeScript, Bun, Tailwind CSS v4, shadcn/ui, Jotai, TanStack Table, React Hook Form, Zod, Sonner, and Lucide React. Install shadcn components through `bunx shadcn@latest add <component>` and prefer existing dependencies over hand-written control primitives.

## Guidelines Index

| Guide | Description | Status |
| --- | --- | --- |
| [Desktop Application Contract](./application-contract.md) | Tauri API, state ownership, events, validation, and UI tests | Active |
| [Invoice Workspace Contract](./invoice-workspace-contract.md) | Paid-import confirmation, invoice task review, ledger controls, and preview behavior | Active |
| [Matching Workspace Contract](./matching-workspace-contract.md) | Synchronized comparison, conflict resolution, custom selection, and confirmation UX | Active |
| [Project Workspace Contract](./project-workspace-contract.md) | Project operations, invoice membership, ordering, export, and overwrite confirmation | Active |
