# TimberStat

A native disk analyzer and file-management suite for Windows 10/11 (primary) and Linux (secondary),
written in Rust with [GPUI](https://www.gpui.rs/) and [GPUI Kit](https://gpui-kit.com/).

## Status

**Phase 2 in progress — the app scans volumes.** The window lists your drives. Choose one to
scan: it shows live progress, can be cancelled, and reports files, folders, logical size and
whether the totals are complete. Physical sizes are not yet reported on Windows (the NTFS path
is Phase 3). Plan and design are in [`docs/IMPLEMENTATION_ROADMAP.md`](docs/IMPLEMENTATION_ROADMAP.md)
and [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md). No file operations, visualizations or
treemap exist yet.

## Requirements

- Rust stable, 1.95 or newer (`rust-version` in `Cargo.toml`; verified in CI).
- Windows 10/11 or Linux with a windowing system and a GPU-capable driver.

## Build and run

```sh
cargo run -p hmstd-app
```

Run the checks CI runs:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Configuration

The configuration file lives in the platform config directory (on Windows,
`%APPDATA%\TimberStat\config\config.json`). It is versioned (`schema_version`).

| Key | Values | Default |
|---|---|---|
| `schema_version` | `1` | `1` |
| `theme` | `system`, `light`, `dark` | `system` |

Override a value for a single run with an environment variable:

```sh
HMSTD_THEME=dark cargo run -p hmstd-app
```

Log level: `HMSTD_LOG=debug cargo run -p hmstd-app` (default `info`).

## Layout

| Path | Purpose |
|---|---|
| `crates/core` | GPUI-independent domain: errors, configuration, logging |
| `crates/app` | Desktop shell (GPUI + GPUI Kit) |
| `docs/` | Architecture, research, roadmap, test and performance plans |
| `Plan.md` | Approved product plan |

## License

See [`LICENSE`](LICENSE).
