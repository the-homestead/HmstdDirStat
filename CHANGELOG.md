# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/). Entries are generated from Conventional Commits
when release automation is in place (Phase 8); until then they are maintained by hand.

## [Unreleased]

### Added
- Volume scanning from the window: pick a drive, see live progress, cancel, and read a summary
  (files, folders, logical size, completeness). Scanning runs off the UI thread.
- `hmstd-filesystem`: arena-based filesystem model with subtree totals, hard-link identity, and
  explicit completeness flags.
- `hmstd-scanner`: metadata-only generic traversal with cancellation, progress, exclusions, and
  partial-failure reporting; Windows drive and Linux mount-point discovery.
- Cargo workspace with `hmstd-core` (GPUI-independent) and `hmstd-app` (GPUI Kit shell).
- Versioned configuration (`schema_version` 1) with defaults, an optional JSON file, and
  `HMSTD_*` environment overrides; atomic saves; corrupt and unsupported files are rejected
  and never overwritten.
- Desktop window showing the active theme, with a persisted light/dark toggle.
- Tracing-based logging, level set by `HMSTD_LOG`.
- Architecture, research, roadmap, dependency, platform, performance, and testing documents in `docs/`.
- CI: formatting, clippy (warnings denied), tests on Windows and Linux, declared-MSRV check, dependency policy.
