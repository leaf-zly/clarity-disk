# Changelog

All notable changes to this project will be documented in this file.

The format follows Keep a Changelog, and the project uses Semantic Versioning.

## [Unreleased]

## [0.1.1] - 2026-09-09

### Added

- Opt-in preview online updates with pinned Minisign verification, download progress, explicit installation confirmation, and Windows restart.
- Native update installation gate that refuses shutdown during tracked file operations and administrator maintenance.
- GitHub-hosted signed-updater preview publication with immutable version assets and a separate mutable update manifest.

### Fixed

- Stable release metadata ignores legacy preview tags and update-feed releases.
- Native titlebar theme permissions, recovery refresh/partial restore handling, and misleading settings-save errors (carried forward from the previous preview).

## [0.1.0] - 2026-08-24

### Added

- Initial Rust, Tauri, Vue, CI/CD, documentation, and dashboard foundation.
- Independent, administrator-protected backup restore receipts bound to another physical disk.
- Disposable VHDX partition fault-lab workflow for dedicated self-hosted Windows runners.
- Activity history filtering, privacy-safe export, and explicit local-summary clearing.
- Versioned settings, fixed-tier quarantine policy, login startup, and read-only automatic maintenance.
- Recovery Center with enumerated alternate destinations and one-time permanent-deletion challenges.
- Official GitHub Release update checks, privacy-safe crash markers, and fixed performance baselines.
- Authenticode-required GitHub releases with publisher verification, installer lifecycle smoke tests, SHA-256 checksums, and provenance attestations.
