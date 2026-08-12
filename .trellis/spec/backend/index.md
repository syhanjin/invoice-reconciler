# Backend Development Guidelines

> Executable conventions for the Tauri/Rust application backend.

## Overview

The backend is a Tauri v2 Rust process. It owns SQLite through SeaORM, managed screenshot storage, the bounded Ollama queue, validation, and all durable state transitions. Read the import contract before changing any cross-layer command or OCR behavior.

## Guidelines Index

| Guide | Description | Status |
| --- | --- | --- |
| [Order Import Contract](./order-import-contract.md) | Commands, OCR, queue, files, validation, and required tests | Active |
| [Database Guidelines](./database-guidelines.md) | SeaORM entities, migrations, transactions, and compensation | Active |
| [Linux Runtime Contract](./linux-runtime-contract.md) | WebKit startup environment and platform-scoped bootstrap rules | Active |

All documentation in this directory is written in English.
