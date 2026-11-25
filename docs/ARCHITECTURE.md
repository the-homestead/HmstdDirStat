# ARCHITECTURE — HmstdDirStat

**Status:** Phase 0/1 in progress. This document describes the target architecture and the
rules that bind it. **No scanner, no file operations, and no visualizations exist yet** —
nothing in this document should be read as a claim that a subsystem is implemented.

HmstdDirStat is a native disk-analyzer and file-management suite for Windows 10/11
(primary) and Linux (secondary), written in Rust with GPUI for the interface and GPUI Kit
for components. The architecture exists to satisfy one product promise: **scan once, then
analyze, visualize, filter, and manage against a single authoritative filesystem model
without ever rescanning.**

---

## 1. Design principles

These rules are binding on every crate and every change. They are derived from the
approved product plan and are repeated here because most architectural decisions are
consequences of them.

1. **One authoritative filesystem model.** Every feature — treemap, radial, file tree,
   search, duplicates, insights, comparison, export — reads the same in-memory model.
   No feature keeps a private, divergent copy of the filesystem.
2. **Changing visualization, filter, or workspace never rescans.** View state is derived
   from the model; only an explicit user scan/refresh mutates the model.
3. **No blocking filesystem work on the GPUI main thread.** All I/O and CPU-heavy work
   runs on dedicated workers; the UI thread only renders and forwards intents.
4. **Scanner and visualization are independent of GPUI.** They are plain Rust crates with
   no UI dependency, so they are testable headless and reusable by the CLI.
5. **File operations are independent of visualization.** The file manager and its
   operations depend on the model and platform abstractions, never on a view.
6. **Platform APIs sit behind abstractions.** `platform-windows` / `platform-linux`
   implement shared traits; shared crates never call Win32 or Linux APIs directly.
7. **Bounded concurrency with cancellation, progress, and backpressure** for every
   expensive operation. No unbounded task spawning, no unbounded channels.
8. **Compact indexed storage for millions of entries.** Arena + numeric ids, not an
   object graph with per-node `Arc` allocations.
9. **No fake controls, no invented benchmarks.** A control exists only if it works; a
   benchmark number exists only if it was measured and is reproducible.
10. **No destructive operation without explicit user intent.** Confirmation, preview,
    and reversible-by-default policies are part of the operation contract.
11. **No hardlink double counting in physical accounting.** Physical (allocated) totals
    count each `(volume, file id)` / `(dev, ino)` once; logical totals are per-entry.
12. **All persistent schemas are versioned** with forward migration; an interrupted save
    must never corrupt a previously saved scan.

---

## 2. Workspace and crate layout

The workspace is created **incrementally**: a crate exists only when it has real code.
Phase 1 contains the minimum set — `app` (GPUI shell), `core` (shared errors/config/
domain primitives, only if actually used), and `configuration` if needed. Empty crates
are not created speculatively.

Target crate boundaries (created as their phase needs them):

```
crates/
  app/                  GPUI shell: window, theme, panels, command palette
  core/                 shared errors, config primitives, domain types (only if used)
  configuration/        versioned user configuration load/save
  platform/             platform abstraction traits (drive, volume, file id, ops)
  platform-windows/     Win32/WinRing3 implementations (volume, USN, file ids, shell)
  platform-linux/       Linux implementations (mounts, statx, trash, permissions)
  filesystem/           FileTree arena, NodeId, aggregation, hardlink identity
  scanner/              generic cross-platform traversal engine (Phase 2)
  scanner-generic/      generic per-platform enumeration strategies
  scanner-ntfs/         NTFS fast path: FSCTL_ENUM_USN_DATA (Phase 3)
  visualization/        layout/view state, shared selection/navigation, render traits
  visualization-treemap/ squarified treemap (first renderer)
  visualization-radial/  radial sunburst (second renderer)
  visualization-advanced/ circle packing, icicle, heatmap, timeline, …
  file-tree/            virtualized tree view, navigation, context menus
  file-operations/      copy/move/delete/rename/restore with safety contract
  hashing/              CRC32/MD5/SHA/BLAKE3/xxHash behind one trait
  duplicates/           staged size→sample→hash duplicate detection
  archive/              archive browse/extract/create via library + external handlers
  search/               query parsing, filters, incremental results
  storage-insights/     rule-based classification and recommendations
  scan-history/         saved scans, comparison, growth tracking
  database/             SQLite per-scan persistence, WAL, migrations
  cli/                  headless scan/export/hash/duplicate/compare
```

Dependency direction (arrows point “depends on”):

```
app ──▶ visualization-* ──▶ visualization ──▶ filesystem
  │            │                                   ▲
  │            ▼                                   │
  ├──▶ file-tree ──▶ file-operations ──────────────┤
  ├──▶ search / duplicates / hashing / archive /   │
  │    storage-insights / scan-history ────────────┤
  ├──▶ scanner ──▶ scanner-generic / scanner-ntfs ─┤
  ├──▶ database ◀── scan-history                    │
  └──▶ core / configuration                         │
                                                    │
platform-windows / platform-linux ──▶ platform ────┘
```

Rules that follow from the arrows:

- `filesystem`, `scanner*`, `visualization`, `platform*` **never** depend on `app` or
  any GPUI crate.
- `file-operations` depends on `filesystem` + `platform`, never on `visualization*`.
- `scanner-ntfs` and `scanner-generic` are selected at runtime behind a `Scanner`
  trait; neither is a hard dependency of the other.
- `core` stays tiny; if it would only hold one type, that type lives with its owner.

---

## 3. The authoritative filesystem model

A single arena-based tree, `filesystem::FileTree`, is the only filesystem representation.

```
FileTree {
    nodes: Vec<Node>,          // arena; index == NodeId
    roots: Vec<NodeId>,        // one per scanned volume/root
    by_parent: …               // child index (first_child / next_sibling)
    hardlink_index: HashMap<FileId, NodeId>,   // first occurrence wins for physical
    scan: ScanProvenance,
}
Node {
    id: NodeId,                // u32, stable for the life of the tree
    parent: Option<NodeId>,
    first_child: Option<NodeId>,
    next_sibling: Option<NodeId>,
    name: Box<str>,            // or Arc<str> when interning proves worthwhile
    kind: NodeKind,            // File | Directory | ReparsePoint …
    logical_size: u64,         // apparent size, summed per-entry
    allocated_size: u64,       // size-on-disk for this entry
    file_id: FileId,           // (volume, file id) Windows / (dev, ino) Linux
    metadata: Metadata,        // timestamps, attributes, permissions (platform-neutral)
    scan_version: u32,         // which scan produced this node
}
```

Hardlink and accounting contract:

- **Logical accounting** sums `logical_size` per entry — every path counts.
- **Physical accounting** sums `allocated_size` once per distinct `file_id`; subsequent
  links contribute 0 to physical totals but still appear as entries.
- Links (symbolic, junction, reparse) are **never followed and never cross mounts** by
  default; they are recorded as `ReparsePoint` nodes with their target metadata only
  when the platform can supply it without traversal.
- `scan_version` + `ScanProvenance` (method: Generic | NtfsUsn, completeness:
  Complete | Partial | Incomplete, started/finished, error summary) travel with the
  tree so the UI can always show *how* and *how completely* the data was gathered.

Aggregation is incremental: directory totals are computed as nodes arrive and finalized
when a directory closes, so a scan never needs a second full pass for sizes.

---

## 4. Scanning

Two scanner implementations behind one `Scanner` trait; selection is a runtime decision
recorded in `ScanProvenance`.

### 4.1 Generic traversal (Phase 2)

Cross-platform, depth-first with a bounded work queue. Yields metadata-only events
(path, kind, sizes, timestamps, attributes, file id) in batches — **file contents are
never read**. Supports cancellation, live progress, per-directory error capture (errors
are data, never silently dropped), and exclude rules.

### 4.2 NTFS fast path (Phase 3)

On NTFS volumes, enumerate the MFT via `FSCTL_ENUM_USN_DATA` to obtain
`(FileReferenceNumber, ParentFileReferenceNumber, name)` records, reconstruct the
hierarchy from parent FRNs, and resolve sizes/attributes with `OpenFileById` +
`GetFileInformationByHandleEx`. Benefits: enumeration independent of directory
traversal order, WizTree-class throughput. Requirements and limits:

- Read-only access; no raw volume writes, ever.
- If the API is unavailable, access is denied, or the volume is not NTFS, the scanner
  **detects the limitation, explains it, and falls back to generic traversal** — the
  fallback is a first-class path, not an error.
- Records that cannot be resolved (deleted/stale FRNs, access denied) are reported as
  incomplete with an accurate reason; the scan is marked `Partial`/`Incomplete`.

### 4.3 Concurrency, progress, backpressure

- Workers are spawned per scan with a **bounded** parallelism budget (small, fixed,
  configurable); no unbounded `spawn` per directory.
- Progress and events flow to the UI over **bounded channels**; when the UI is slow the
  scanner applies backpressure (batch coalescing) rather than growing memory without
  bound.
- Cancellation is cooperative and checked at batch boundaries; a cancelled scan
  produces a tree marked incomplete, never a silently truncated “complete” one.
- GPUI executors are used for UI-affine work; Tokio/Rayon are deferred until a concrete
  need appears, and then only behind the scanner/worker boundary — never on the UI
  thread.

---

## 5. Visualization

`visualization` owns shared state — selection, navigation, filter, color config, zoom,
breadcrumbs — and a `Renderer` trait. Each `visualization-*` crate implements the trait;
the app hosts one or more panes. **Switching renderer, filter, or workspace is a pure
view-state change over the existing tree.**

Rendering contract:

- Custom GPUI drawing (not one GPUI element per file). Stable node keys (`NodeId`) for
  incremental updates and selection sync across panes.
- **Visible-primitive caps**: only nodes intersecting the viewport and above a size
  threshold are drawn; the rest are folded into aggregate tiles (“… N more items”).
- Layout algorithms: **squarified treemap first**, then radial sunburst; slice-and-dice,
  binary, circle packing, icicle, heatmap, timeline follow. Layout math lives in the
  crate, testable without a window.
- Proportional area uses the selected size metric (logical/allocated); zero-byte and
  tiny files are handled explicitly (minimum tile + count badge), never dropped silently.

---

## 6. File operations

`file-operations` implements open/copy/move/rename/delete/restore/create/shortcut/
properties against `platform` traits, consuming `NodeId`s from the tree. Contract:

- Destructive operations require explicit user intent and a preview; collisions offer
  skip / overwrite / rename / keep-both; **never silently overwrite**.
- Progress + safe cancellation + partial-failure reporting are part of every operation.
- Independent of any visualization; the file tree view is one possible caller.

---

## 7. Persistence and automation

- Saved scans: **SQLite, one database per scan, WAL mode**, published transactionally
  (write to temp → atomic rename) so an interrupted save cannot corrupt prior scans.
  All schemas are versioned with migration; the tree is serialized with its
  `ScanProvenance`.
- Exports: JSON and CSV. Import of *untrusted* SQLite files is not supported directly;
  only app-produced, schema-matched databases are opened, and foreign files are
  rejected with a clear message.
- `cli` reuses `scanner` + `filesystem` + `hashing`/`duplicates` headlessly; it is a
  consumer of the same crates, not a parallel implementation.

---

## 8. Platform abstraction

`platform` defines traits: `VolumeEnumerator`, `FileSystemScanner` (metadata ops),
`FileIdentity`, `FileOperations`, `Trash`, `ShellIntegration`. `platform-windows`
implements them with Win32 (volume discovery, `OpenFileById`, USN, Recycle Bin, shell
associations); `platform-linux` with `/proc/mounts`, `statx`, `trash`, permissions.
Shared crates see only the traits. Platform capability differences are surfaced as
`PlatformCapability` flags so the UI can disable/annotate controls instead of failing.

---

## 9. Errors, configuration, logging

- `core` (or the owning crate) defines `thiserror` domain errors per subsystem;
  `anyhow` only at the app boundary. Filesystem errors are values (per-path), never
  panics and never silent.
- `configuration`: versioned, serde-based user config (theme, palette, exclusions,
  scan profiles, workspace layouts) with forward migration; a config from a newer
  version is rejected with a clear message, not partially applied.
- `tracing` structured logging throughout; a log file per session under the app data
  dir; sensitive paths redacted where feasible.

---

## 10. Risks and mitigations

| Risk | Mitigation |
| --- | --- |
| USN enumeration blocked by permissions or non-NTFS volume | Explained fallback to generic scanner; scan marked with method + completeness |
| GPUI main-thread stalls on large trees | Renderer caps + folded aggregates; all I/O off-thread; bounded channels |
| Memory growth on tens of millions of entries | Arena + u32 ids + boxed small strings; backpressure; incremental aggregation |
| Hardlink double counting | `FileId` index; physical totals count first occurrence only |
| Saved-scan corruption on crash | SQLite WAL + transactional publication + schema versioning |
| Platform API leakage | Traits in `platform`; capability flags; CI builds on Windows + Linux |
| Scope creep into empty crates | Crates created only when they carry code |