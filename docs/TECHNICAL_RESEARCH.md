# TECHNICAL RESEARCH — TimberStat

**Status:** Phase 0 research record. Dated 2026-11-25. Versions are crates.io snapshots,
not guarantees of being latest. Items marked **UNVERIFIED** were reported by research but
not confirmed against a primary source by the author and must be checked before they drive a decision.

---

## 1. UI framework: GPUI and GPUI Kit

| Fact | Status | Source |
|---|---|---|
| `gpui-kit` latest is 0.11.0 (published 2026-11-23) | Verified (crates.io index read locally) | https://crates.io/crates/gpui-kit |
| `gpui-kit` 0.11.0 depends on `gpui-pre =0.3.14` (`package = "gpui-pre"`) and `gpui_platform` = `gpui-pre-platform =0.3.14` | Verified (`Cargo.toml` of the unpacked crate) | crate `Cargo.toml` |
| `gpui-kit` 0.11.0 is edition 2024 | Verified | crate `Cargo.toml` |
| Umbrella re-exports `gpui`, `platform`, `base`, `component`, `assets`; `init`, `open_window`, `application` | Verified (`src/lib.rs`) | crate source |
| Minimal app: `application().with_assets(assets::Assets).run(|cx| { init(cx); open_window(...) })` | Verified (upstream hello_world example) | https://github.com/longbridge/gpui-kit/blob/main/examples/hello_world/src/main.rs |
| Theme: `Theme::update`, `Theme::change(ThemeMode::Dark/Light)`, `Theme::sync_system_appearance`, `cx.theme()` | Verified (`gpui-component` `theme/mod.rs`) | crate source |
| crates.io `gpui` 0.2.2 is a separate, older line; not the Kit-compatible choice | Verified by `cargo info` | crates.io |
| Upstream CI tests on `x86_64-windows-msvc`, `x86_64-linux-gnu`, `aarch64-apple-darwin` | Verified | https://github.com/longbridge/gpui-kit/blob/main/.github/workflows/ci.yml |
| GPUI does not use Tokio; it has its own foreground (main thread) and background executors | `cargo tree -p timberstat-app -e normal` (host target, Windows) lists no `tokio`; `cargo tree -i tokio` prints nothing. `gpui-pre` exposes `BackgroundExecutor` (`background_executor()`), which the app uses in `crates/app/src/shell/scanning.rs` and `fileops_ui`. `tokio` appears in `Cargo.lock` only through `gpui-pre-reqwest`, a wasm-only dependency of `gpui-kit-assets` | **Verified** for the native build. The `tokio runtime` comment in `gpui-pre/src/app.rs` was not traced to runtime behaviour |
| Declared `rust-version` for gpui-pre/gpui-kit is absent; effective floor reported as 1.95 via a transitive dependency (`oo7`) | **UNVERIFIED** | must be checked by a clean build on Rust 1.95 |

**Decision:** use the `gpui-kit` umbrella (0.11.0) so the app depends on one crate and the
GPUI family stays version-locked. Do not depend on `gpui` 0.2.2 directly.

**Open risk:** GPUI Kit's own Windows CI is a test job, not a release channel; the app's
Windows build must be proven locally and in our CI.

## 2. Filesystem scanning

### 2.1 Windows NTFS fast path (Phase 3)

- `FSCTL_ENUM_USN_DATA` enumerates MFT-backed records through a volume handle. Requires an NTFS
  volume, and in practice elevation for raw volume access. Unsupported on SMB; CSVFS supported.
  https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ni-winioctl-fsctl_enum_usn_data
- Records (`USN_RECORD_V2/V3/V4`) carry file reference number (FRN), parent FRN, name, timestamps,
  attributes. **They do not carry sizes.** Sizes come from a per-file open by ID:
  `OpenFileById` + `GetFileInformationByHandleEx(FileStandardInfo)` (`AllocationSize`, `EndOfFile`,
  `NumberOfLinks`). https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-openfilebyid
- MFT enumeration is **not an atomic snapshot**. Concurrent create/delete/rename can produce missing,
  stale, or duplicate entries. A scan that observes this must be marked partial.
- Deleted MFT entries are marked free and may be reused; enumeration is not a deleted-file audit.
- Reparse points: `FSCTL_GET_REPARSE_POINT` / `FileAttributeTagInfo`. Never follow by default.
- Hard links: one physical storage charge per `(volume serial, file id)`; logical per-path totals are separate.

### 2.2 Generic traversal (Phase 2, all platforms)

- `symlink_metadata` for entries, so links are not followed; `metadata` only when target is wanted.
- Linux: `std::os::linux::fs::MetadataExt` — `st_blocks` (512-byte units) gives an allocated-size
  approximation (`st_blocks * 512`); not physical truth for sparse, compressed, CoW or network filesystems.
- Identity: `(st_dev, st_ino)` on Linux; `(volume serial, file id)` on Windows.
- Mount boundary: stop at a `st_dev` change unless the user opts in.

### 2.3 Crates considered for scanning

| Crate | Use | Decision |
|---|---|---|
| `windows` / `windows-sys` | Win32 bindings for USN, FSCTLs, `OpenFileById` | Adopt in Phase 3 (platform crate only) |
| `walkdir` | Serial traversal reference | Candidate for Phase 2 fallback/tests |
| `jwalk` | Parallel traversal (Rayon) | Deferred; bounded custom pool preferred |
| `ignore` | gitignore-style exclusions | Not adopted: hidden-file default is wrong for a disk analyzer; use own exclusion predicate |
| `ntfs-mft`, `usn-journal-rs` | Pre-built NTFS readers | **Evaluate only in Phase 3**; the project owns its parser unless one passes audit (fuzzing, maintenance, license) |

## 3. Reference products

| Project | Observed (from its source, per research) | Relevance |
|---|---|---|
| WizTree (`diskanalyzer.com`) | Closed source; MFT-based speed claim | Target behaviour only; no numbers reused |
| MacDirStat (`github.com/MichaelStromberg/macdirstat`) | Compact `FileNode` tree, per-thread extension aggregation, cushion treemap, hardlink dedupe via sharded inode set | Model and treemap-shading ideas; egui-specific code not reused |
| SquirrelDisk (`github.com/adileo/squirreldisk`) | Arena `Vec<Node>` with `u32` indices, small-file folding, squarified treemap with minimum-area folding, sunburst with minimum arc, stable keys for morphing | Closest model to our `FileTree`; design reference |

These were read through research-agent reports, not reviewed line-by-line by the author. Re-verify
specific claims before citing them in code comments.

## 4. Visualization algorithms

| Algorithm | Primary source |
|---|---|
| Squarified treemap | Bruls, Huizing, van Wijk, "Squarified Treemaps" (2000) — https://vanwijk.win.tue.nl/stm.pdf |
| Ordered / quantum treemaps | Bederson, Shneiderman, Wattenberg (2002) — http://www.cs.umd.edu/~ben/papers/Bederson2002Ordered.pdf |
| Sunburst / focus+context | Stasko, Zhang (2000) — https://people.cs.vt.edu/~north/infoviz/sunburst.pdf |
| Cushion shading | van Wijk & van de Wetering (1999) cushion treemaps; WinDirStat implementation as reference |

Proposal: squarified first (best aspect ratios); slice-and-dice and binary as alternatives behind the same
layout trait; radial second. Layout is pure Rust, no GPUI types (see ARCHITECTURE §7).

## 5. Data model

- One arena: `Vec<Node>`, `NodeId(u32)`, child list via `first_child` / `next_sibling` (or a compact child
  index). Strings interned or stored as `Box<str>`. Aggregates (logical, allocated, file count) updated
  incrementally on insert and rolled up post-scan.
- Hardlink identity held in a sharded set keyed on `(volume, file id)`.
- Stable keys for animation and selection: node id for entries, `(parent, group-flag)` for folded aggregates.
- Scale target only; no measured capacity claim until Phase 3 benchmarks exist.

## 6. Persistence (Phase 7)

- SQLite via `rusqlite` (candidate 0.43.0, `backup` + `wal`): one database per saved scan + catalog.
- WAL, `foreign_keys=ON`, `trusted_schema=OFF`; publish a scan only after a complete commit so an interrupted
  save never corrupts an earlier scan.
- Versioned schema with `PRAGMA user_version` or a migrations table; each migration in its own transaction.
- Import/export via an app-defined, checksummed format; never open untrusted SQLite files as live databases.

## 7. Hashing and archives (Phase 6)

- Hashes: `sha2` (SHA-256/512), `blake3` for fast strong hashing, `crc32fast` as checksum only, `md-5`/`sha1` legacy
  compatibility only. Staged duplicate detection: size → sample → full hash → byte comparison before any destructive action.
- Archives: `zip`, `tar` (+ `flate2`, `bzip2`, `zstd`), `xz2` flagged for maintenance risk. 7z and RAR are
  evaluation items: open-source RAR write is not available. Extraction must reject path traversal and bomb ratios.

## 8. Open questions

1. GPUI Kit minimum Rust version: verify 1.95 by a clean build (see DEPENDENCY_SELECTION §4).
2. Whether `ntfs-mft` is acceptable after audit, or the NTFS parser is owned in-tree.
3. Licence for the project (not chosen; see README).
4. Target GPU/driver floor for GPUI rendering on Windows (see PLATFORM_COMPATIBILITY §3).
