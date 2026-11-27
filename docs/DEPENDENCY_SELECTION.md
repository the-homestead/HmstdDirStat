# DEPENDENCY SELECTION — TimberStat

Rule (Plan §23.12): no unnecessary dependencies. A crate enters the tree only when a
milestone in IMPLEMENTATION_ROADMAP.md needs it. Versions are crates.io snapshots read
2026-11-25; exact pins are fixed by `Cargo.lock`, which is committed.

## 1. Accepted for Phase 1 (the shell)

| Crate | Version | Why it is needed now |
|---|---:|---|
| `gpui-kit` | 0.11.0 | The approved UI stack (GPUI + GPUI Kit). Single dependency; pins GPUI family |
| `serde` | 1.0.229 | Versioned configuration file |
| `serde_json` | 1.0.154 | Human-readable config format |
| `tracing` | 0.1.44 | Structured logging (Plan §2) |
| `tracing-subscriber` | 0.3.23 | Console/file log output |
| `thiserror` | 2.0.21 | Typed domain errors (Plan §2) |
| `anyhow` | 1.0.105 | Application-boundary error propagation (Plan §2) |
| `directories` | 6.0.0 | Platform config/data directories (Windows Known Folders, XDG) |

Dev-only (Phase 1 tests): `tempfile` 3.27.0 (filesystem fixtures).

Not added in Phase 1 even though the Plan lists them: `tokio`, `rayon`, `criterion`, `proptest`.
Each returns when a milestone needs it: Rayon/Tokio for the scanner (Phase 2), Criterion for
benchmarks (Phase 3), proptest for duplicate/path logic (Phase 6).

## 2. Deferred, with the phase that adopts them

| Crate | Version | Phase | Note |
|---|---:|---|---|
| `windows` or `windows-sys` | current | 3 | NTFS USN/FSCTL bindings; platform crate only |
| `walkdir` | current | 2 | Test oracle and fallback reference |
| `rayon` | 1.12.0 | 2 | Bounded custom pool only, never implicit global |
| `tokio` | 1.54.1 | when needed | Not needed until an async I/O requirement is proven |
| `rusqlite` | 0.43.0 | 7 | `backup`, `wal` features |
| `criterion` | 0.8.2 | 3 | In-process microbenchmarks |
| `proptest` | 1.12.0 | 6 | Path, grouping and diff invariants |
| `sha2` | 0.11.2 | 6 | SHA-256/512 |
| `blake3` | 1.8.7 | P4 (accepted, section 8) | Fast strong hash |
| `crc32fast` | 1.5.6 | 6 | Checksum only |
| `md-5` / `sha1` | 0.11.0 | 6 | Legacy compatibility only |
| `xxhash-rust` | 0.8.21 | 6 | Non-cryptographic pre-screen |
| `zip` | 9.8.2 | 9 | Archive support |
| `tar` / `flate2` / `zstd` / `bzip2` | 0.4.46 / 1.1.11 / 0.14.5 / 0.6.1 | 9 | Compressed tar family; native C builds in CI |
| `xz2` | 0.1.7 | 9 | **Maintenance risk** (last registry update 2022); re-evaluate before use |
| `sevenz-rust2` | 0.25.0 | 9 | Evaluate; community fork |
| `git-cliff` | 2.14.2 | 8 | Release-notes tool (dev/CI) |
| `cargo-release` | 1.1.7 | 8 | Version bump/tag mechanics (dev); needs Rust 1.97 on the release machine |
| `cargo-deny` | 0.22.1 | 1 CI | Licence/advisory/ban policy |
| `cargo-audit` | 0.23.0 | 1 CI | RustSec advisories |

### Logging and performance tooling: decision deferred

- Logging stays on `tracing` + `tracing-subscriber` for Phase 1 (already in use).
- `slog` (https://lib.rs/crates/slog) is a candidate alternative. Not adopted; comparison is
  deferred by owner instruction until the logging and performance-measurement needs of Phase 2–3 are concrete.
- `tracing` (https://lib.rs/crates/tracing) is also the instrumentation point for performance spans
  in Phase 3; any profiler/benchmark crate choice is deferred with it.

## 3. Rejected

- `gpui` 0.2.2 from crates.io: separate, older line; conflicts with the Kit-pinned GPUI family.
- `egui`: reference projects use it; not the approved stack (Plan §2).
- `ignore`: hidden-file default unsuitable for a disk analyzer.
- `jwalk`: parallel traversal; rejected for now in favour of an owned bounded pool so cancellation and progress are ours.
- Any ORM: SQLite via `rusqlite` directly is enough (Phase 7).

## 4. Toolchain

- Local: `rustc 1.100.0` stable MSVC (verified).
- Declared: `rust-version = "1.95"` in workspace; `rust-toolchain.toml` pins `stable`.
- **Unverified:** that 1.95 is the true floor of the GPUI Kit tree. Verify with a clean build on
  Rust 1.95 before the MSRV claim is treated as fact. Until then the CI job uses stable only.

## 5. Licence and publishing

Not decided. No `LICENSE` file is committed until the owner chooses one. Package scope and
publish channel are not chosen (owner instruction).

## 6. blessed.rs survey (owner request, 2026-11-25)

Source: https://blessed.rs/crates (Common, CLIs, Concurrency, Graphics, System, Tooling sections).
Rule: a crate is adopted only when the **adopt-when** milestone starts. Nothing below is added in Phase 1
beyond what §1 already lists.

| Need (phase) | Candidate(s) on blessed.rs | Decision |
|---|---|---|
| Regular expressions (6, Plan §10) | `regex` (`fancy-regex` only if backtracking is required) | Adopt in Phase 6 |
| Glob exclusions (2, Plan §4.3) | `globset` | Adopt in Phase 2; preferred over `ignore`, whose hidden-file default is wrong here |
| Directory walk (2) | `walkdir` | Adopt in Phase 2 (already §2) |
| Bounded channels (2) | `crossbeam-channel`, `flume` | Adopt in Phase 2 only if `std::sync::mpsc` proves insufficient (select needed) |
| Sharded hardlink set (2–3) | `dashmap`, `papaya` | Evaluate in Phase 3 with profiling; a small in-tree sharded map is also acceptable |
| Parallel work (2, Plan §2) | `rayon` | Already approved (§2 deferred list) |
| Mutex / atomic swap (2+) | `parking_lot`, `arc-swap` | Defer; `std` suffices until profiling says otherwise |
| Safe byte casts for USN/MFT records (3) | `zerocopy`, `bytemuck` | Adopt in Phase 3 if raw record parsing is in-tree |
| Bit flags for file attributes (2–5) | `bitflags` | Adopt in Phase 2 when attribute models are written |
| Ordered maps (4–5) | `indexmap` | Defer until a concrete ordering need exists |
| Windows API (3, 5, 8) | `windows` (official Microsoft crate) | Adopt in Phase 3 (platform-windows crate only) |
| Linux syscalls (2–3) | `rustix` (preferred over `nix`), `libc` | Adopt only if std is insufficient for a specific call |
| Ctrl+C / signal cancel for CLI (7) | `signal-hook` | Adopt in Phase 7 |
| Clipboard: copy path/name (5, Plan §7) | `arboard` | Adopt in Phase 5 |
| Native file open/save dialogs (5, Plan §17) | `rfd` | Adopt in Phase 5 after GPUI window-handle compatibility is verified |
| File watching for refresh (5) | `notify` | Defer; only if a required feature needs live refresh |
| Timestamps display (5, Plan §7) | `jiff` (blessed prefers it), `time`; `chrono` is soft-deprecated | Decide in Phase 5 |
| CLI parsing (7, Plan §16) | `clap` (fully featured, slow compile); `bpaf`, `lexopt`, `pico-args` | Decide in Phase 7; `clap` is the default unless compile time is a blocker |
| CLI progress / colour (7) | `indicatif`, `termcolor` | Adopt in Phase 7 |
| CLI interactive confirm (7) | `inquire` | Adopt in Phase 7 only if a CLI prompt is needed; destructive ops need explicit flags regardless |
| Snapshot tests (1–7) | `insta` | Adopt when the first output format needs snapshotting (Phase 2 reports or Phase 7 CLI) |
| Test runner (1) | `cargo-nextest` (dev tool) | Optional now; install locally, not a dependency |
| Benchmarks (3, Plan §18) | `criterion` (Plan), `divan` (allocation profiling) | Criterion stays per Plan; `divan` evaluated in Phase 3 for allocation counts |
| Heap / CPU profiling (3, Plan §18) | `dhat`, `cargo-flamegraph`, `cargo-show-asm` | Dev tools in Phase 3 |
| Release automation (8) | `cargo-release`, `release-plz` (blessed lists both) | Already deferred (§2, §5 above); owner decides |
| Semver checks (8) | `cargo-semver-checks` | Only if crates are published; `publish = false` now, so not needed |
| CI actions (1) | `dtolnay/rust-toolchain`, `Swatinem/rust-cache`, `taiki-e/install-action`, `cargo-machete` (upstream GPUI Kit CI uses it) | Adopt in Phase 1 CI |
| Dependency audit/policy (1) | `cargo-deny`, `cargo-audit`, `cargo-license` | `deny` and `audit` in Phase 1 CI; `license` optional |
| Once-cell / lazy_static | `std::sync::LazyLock` (stdlib) | Use std; do not add the crates |
| Random numbers | `rand`, `fastrand` | Not needed by any approved feature |
| UUIDs | `uuid` | Defer; `FileId`/`NodeId` are internal integers |
| Web/HTTP/gRPC/websockets/databases-as-server | `reqwest`, `ureq`, `axum`, `tonic`, `tungstenite` | Rejected: Plan has no network service; network *paths* are filesystem access |
| Other UI toolkits | `egui`, `iced`, `slint`, `tauri`, `gtk4`, `leptos`, `dioxus` | Rejected: GPUI/GPUI Kit is approved (Plan §2) |
| Renderers / layout / text | `skia-safe`, `vello`, `femtovg`, `taffy`, `parley`, `cosmic-text`, `winit` | Rejected: GPUI owns rendering, layout and windowing |
| TUI | `ratatui`, `crossterm` | Rejected: no TUI in the plan |
| Error reporting | `color-eyre` (blessed alternative to `anyhow`) | Keep `anyhow` (Plan §2); revisit only if user-facing error rendering needs it |
| Compression | `flate2` | Adopt in Phase 9 with archive support (already §2) |
| Hashing / crypto | `blake3`, `sha2`, `argon2`, `zeroize` | `blake3`/`sha2` already planned (Phase 6); `zeroize` only if passwords are handled (Plan §9 archives) |
| Math/numeric/DataFrame | `num-*`, `nalgebra`, `polars`, `datafusion` | Rejected: not required by any phase |
| FFI / bindings | `cxx`, `bindgen`, `pyo3`, `napi`, `uniffi` | Rejected: no foreign-language surface planned |
| Macros | `syn`, `quote`, `darling`, `pastey` | Not needed now; transitive via dependencies only |

Concrete follow-up: none in Phase 1. The adopt-when triggers above become checklist items in IMPLEMENTATION_ROADMAP.md when their phase starts.

## 7. File operations (TimberStat P3, Plan section 7 and 23.11)

Accepted: `trash` 5.4.0 (crates.io, latest release, published 2026-11-17; read 2026-11-26).
Why it is needed now: Recycle Bin delete is the default delete mode (Plan section 7). The crate
wraps the Windows shell delete and the freedesktop trash spec, so no COM code is written here.
It is used only by `timberstat-fileops`, which is GPUI-independent. `cargo deny --all-features check`
passed with it (advisories, bans, licenses, sources). Its MSRV is 1.87, below the workspace 1.95.

Copy, move, and rename use `std` only; no other crate was added for file operations.
Permanent delete uses `std::fs` only. The Recycle Bin call is injectable, so tests never reach the real bin.

## 8. Duplicate detection (TimberStat P4, Plan sections 8 and 20)

Accepted for `timberstat-duplicates` (crates.io, read 2026-11-26). `cargo deny --all-features check` passed with all three.

| Crate | Version | Why it is needed now |
|---|---:|---|
| `blake3` | 1.8.7 | Full-file digest (stage 4). Chosen over `sha2` 0.11.2 because the Plan names BLAKE3 as the fast strong hash and the pipeline needs only an equality test. Every reported group is also byte-verified (stage 5), so the digest is never the sole evidence. Default features; no `rayon` feature. |
| `rayon` | 1.12.0 | Parallel stages on a custom pool built per run, bounded to 1..=4 threads by default. The global pool is never used. |
| `same-file` | 1.0.6 | Hard-link identity on Windows. `MetadataExt::volume_serial_number` and `file_index` sit behind the unstable `windows_by_handle` feature on the workspace toolchain, and the workspace forbids `unsafe`, so the Win32 call cannot be written here. `same_file::Handle` wraps the same call behind a safe, hashable key. Already in Cargo.lock through another crate. |

Decisions taken for P4 (owner can override):

- Pipeline: size, then hard-link identity, then sample (BLAKE3 of the first and last 64 KiB), then full BLAKE3, then byte compare. Only groups that pass every stage are reported.
- Zero-byte files are excluded by default (`min_size` 1). They reclaim nothing.
- Reclaimable space is `(copies - 1) * size`, in logical bytes. It is reported beside the on-disk totals and never merged with them.
- Hard links are detected with a handle per file, held for one size group at a time, so open handles are bounded by the largest size group.
- No copy is marked as the keeper. Deleting goes through the existing delete preview, one copy per action.
- A cancelled run returns no groups, so a partial result is never shown as complete.
- Scope: regular files only. Folders, symlinks and special entries are skipped.
- The duplicate panel is a widget (`WidgetKind::Duplicates`). It starts hidden and joins the tiled layout through View.

## 9. Headless CLI (TimberStat P5.8, Plan section 16)

Accepted: `signal-hook` 0.4.6 with `default-features = false` (crates.io and API docs read 2026-11-26).
The CLI needs safe Ctrl+C delivery to the existing scanner and duplicate cancellation tokens. It uses the
`flag` API, which supports Windows, and an ordinary monitor thread. The `iterator` API is non-Windows and
is therefore not enabled. The crate's MSRV is 1.66, below the workspace 1.95.

Argument parsing uses `std`; no argument-parsing crate was added. No other new dependency was introduced.

