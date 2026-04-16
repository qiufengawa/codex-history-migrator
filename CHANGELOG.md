# Changelog

All notable changes to this project will be documented in this file.

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
