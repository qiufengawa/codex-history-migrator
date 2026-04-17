# Changelog

All notable changes to this project will be documented in this file.

## [1.0.2] - 2026-04-17

### Added

- Chat history management page with metadata filtering, detail preview, archive/unarchive, selected export, trash restore, and trash cleanup
- Direct delete entry from the detail panel and visible batch delete actions in the management toolbar
- Select-all action for the current filtered result set

### Changed

- Refined the management page layout to be more readable and more desktop-tool oriented
- Improved preview summarization for MCP-style JSON titles, event messages, tool results, and long path display
- Added quick-copy actions for thread ID, project path, and payload path in the detail panel

## [1.0.1] - 2026-04-16

### Fixed

- Automatically detects the local Codex directory instead of relying on a username-shaped placeholder path
- Added startup feedback when auto-detection succeeds or when manual `.codex` path input is still needed
- Documented the supported auto-detection environment variables in the README

## [1.0.0] - 2026-04-16

### Added

- Initial public release
- Lightweight Chinese GUI for Codex local history migration
- Export and import workflows for thread metadata, session payloads, and session index
- Optional backup before import, enabled by default
- Provider inspection, one-click provider sync, and latest-backup restore
- Embedded Windows icon and GUI subsystem build without console window
- Progress reporting for scan, export, and import operations

### Security

- Rejects unsafe archive paths during package extraction
- Restricts exported payload files to safe paths under `.codex`
- Validates package manifest and file checksums before import mutation
- Verifies session payload integrity as part of package checksums
