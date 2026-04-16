# Codex History Migrator

<p align="center">
  <img src="assets/app-icon.svg" alt="Codex History Migrator logo" width="140" />
</p>

<p align="center">
  A lightweight Windows desktop tool for migrating Codex local history and syncing thread providers.
</p>

<p align="center">
  <a href="./README.zh-CN.md">简体中文文档</a>
</p>

## What This Is

`Codex History Migrator` is a lightweight Windows desktop utility built with Rust and `egui/eframe`.

It focuses on two practical jobs for local Codex data:

1. Exporting and importing local chat history between different machines or different `.codex` folders.
2. Syncing older threads to the current `model_provider` configured in `config.toml`, so historical threads remain visible under the active provider.

## Core Features

- Chinese-first GUI for direct end-user use
- Automatically detects the local Codex directory using `CODEX_HOME`, `USERPROFILE`, `HOME`, or `HOMEDRIVE` + `HOMEPATH`
- Scan the current `.codex` directory and show a quick overview
- Export migration packages containing:
  - `state_5.sqlite`
  - `sessions/**/*.jsonl`
  - `archived_sessions/**/*.jsonl`
  - `session_index.jsonl`
- Import migration packages into another `.codex`
- Optional backup before import, enabled by default
- Inspect current provider status and thread distribution
- One-click sync of legacy threads to the current provider
- Optional backup before provider sync, enabled by default
- Restore the latest provider-sync backup
- Progress feedback for scan, export, and import operations
- Embedded Windows icon with no extra console window on launch

## Safety and Integrity

This release includes protection for several high-impact failure cases:

- Archive extraction rejects path traversal entries
- Export only packages session payloads inside `.codex/sessions` and `.codex/archived_sessions`
- Import validates `manifest.json`, `db/threads.sqlite`, and `checksums.json` before mutation
- Package contents are verified with SHA-256 checksums, including session payload files

## What It Does Not Migrate

To stay lightweight and predictable, this tool does **not** migrate:

- login state
- API tokens or secrets
- desktop account databases
- plugin installations
- MCP registrations
- unrelated local cache data

## Requirements

- Windows 10/11
- For normal users, download the single-file executable from Releases
- For developers, install the stable Rust toolchain to build from source

## Quick Start

### Download the App

Download the latest single-file Windows executable from [GitHub Releases](https://github.com/qiufengawa/codex-history-migrator/releases):

- `codex-history-migrator-v1.0.1-windows-x86_64.exe`

### Run from Source

```powershell
cargo run --release
```

### Build

```powershell
cargo build --release
```

The executable will be generated at:

```text
target\release\codex-history-migrator.exe
```

## Typical Workflows

### 1. History Migration

1. Open the app and confirm the `.codex` path.
2. Scan the current history from the Overview page.
3. Export a migration package.
4. Open the app on the target machine.
5. Select the package and import it.
6. Keep the backup option enabled unless you explicitly want to skip it.

### 2. Provider Unification

1. Open the Sync page.
2. Check the current provider and distribution of threads.
3. Run one-click sync to the active provider.
4. Restore from the latest backup if you need to roll back.

## Acknowledgements

This project was inspired by the community project [`GODGOD126/codex-history-sync-tool`](https://github.com/GODGOD126/codex-history-sync-tool), but this repository is rewritten in Rust and reimplemented around a lightweight GUI, single-file Windows delivery, and a smoother export/import plus provider-sync workflow.

## License

MIT
