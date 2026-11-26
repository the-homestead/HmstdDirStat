# IMPLEMENTATION ROADMAP — TimberStat

**Status:** Phase 0/1 in progress; no scanner or file operations yet.
This document maps every milestone into independently testable increments with concrete
acceptance gates, dependencies, and explicit non-goals. It follows the 22-phase plan
from `Plan.md` and expands each phase into its own deliverable.

---

## Phase 0 — Research and Architecture *(Deliverable: Approved technical architecture and implementation plan)*

| # | Task | Owner / Dependency |
|---|------|--------------------|
| 0.1 | Inspect reference projects: diskanalyzer.com, macdirstat, squirreldisk | Readers only |
| 0.2 | Inspect GPUI (`https://www.gpui.rs/`) and GPUI Kit (`gpui-kit.com/docs/installation`) | Readers + dev |
| 0.3 | Verify Rust/Cargo versions compatible with gpui-pre and gpui-kit 0.11.0 | Dev |
| 0.4 | Research Windows NTFS MFT enumeration APIs (USN, FRN) | Dev |
| 0.5 | Research Linux filesystem enumeration (statx, mounts, permissions) | Dev |
| 0.6 | Evaluate data structures for millions-of-records arena model | Dev |
| 0.7 | Establish architecture (this doc) | Author |
| 0.8 | Define performance benchmarks (scan startup, throughput, RAM, CPU) | Dev |
| 0.9 | Identify technical risks (permissions, USN blocks, memory growth) | Dev |
| 0.10 | Create implementation specifications | Author |

**Gate:** All three research items (0.4–0.6) completed → architecture approved → proceed to Phase 1.

---

## Phase 1 — Project Foundation *(Deliverable: Buildable native application shell)*

| # | Task | Acceptance Gate |
|---|------|-----------------|
| 1.1 | Initialize Cargo workspace under `A:/Dev/TimberStat/crates/` | `cargo new crates/app && cargo add gpui-gui-base gpui-component gpui-kit` |
| 1.2 | Set up `Cargo.toml`, `rust-toolchain.toml`, `.gitignore`, sample config | `cargo build --release` succeeds on both Windows and Linux targets |
| 1.3 | Implement main window: top toolbar, left sidebar, main workspace, info panel | UI renders without blocking scan workers |
| 1.4 | Theme system: Dark / Light / System / Custom accent | Each theme loads via config; no compile errors |
| 1.5 | Logging & error handling: structured tracing, domain errors, anyhow propagation | `tracing` emits structured logs; `thiserror` types exist |
| 1.6 | Configuration module: versioned serde config (theme, palette, exclusions, profiles) with forward migration | Config v2 accepted by app rejects v1 gracefully |
| 1.7 | Initial CI: formatting (`cargo fmt`), clippy, unit tests, Windows builds, Linux builds | GitHub Actions workflow passes all stages |
| 1.8 | Versioning foundation: git-cliff changelog generation, tagging, release notes | Tag created on first commit |
| 1.9 | First benchmark baseline (small dir, hundreds of k files, tens of m entries) | Baseline numbers recorded, not invented |

**Gate:** Shell runs, builds cleanly on both platforms, CI passes, baseline benchmark exists → Phase 2 start.

---

## Phase 2 — Filesystem Core *(Deliverable: Working cross-platform filesystem scanner)*

| # | Task | Acceptance Gate |
|---|------|-----------------|
| 2.1 | Platform abstraction traits: `VolumeEnumerator`, `FileSystemScanner`, `FileIdentity`, `FileOperations`, `Trash`, `ShellIntegration` | Traits defined; platform modules implement them |
| 2.2 | Generic traversal engine (Phase 2): depth-first, bounded queue, metadata-only events, exclude rules, live progress, cancellation | Scan over 100k random files completes within budget; no file contents read |
| 2.3 | Hardlink identity: `(volume, file id)` counted once physically; logical vs physical accounting separated | Physical total = sum(allocated_size) per distinct file_id; verified against manual count |
| 2.4 | Metadata structures: timestamps, attributes, permissions, reparse points, fragmented metadata | Every node carries complete metadata fields |
| 2.5 | Cancellation & backpressure: cooperative cancel, batch coalescing, partial failure reporting | Cancel mid-scan produces incomplete tree, never truncated complete one |
| 2.6 | Progress reporting: batched UI updates, bounded channels, screen reader friendly | UI shows progress bar per directory, no UI freeze |
| 2.7 | Scanner tests: empty dirs, deep nesting, large datasets, unicode, long paths, permissions, symlinks, junctions, hard links, sparse files, reparse points, changing files, deleted files, network paths, removable drives, interrupted scans | Test suite covers all categories; pass rate ≥ 90% |

**Gate:** Cross-platform generic scanner works on both NTFS and ReFS/FAT32/ext4/Btrfs/XFS; tested on simulated removable drive; no silent drops on deletion/stale records.

---

## Phase 3 — Extreme Performance *(Deliverable: Production-oriented scanning engine)*

| # | Task | Acceptance Gate |
|---|------|-----------------|
| 3.1 | NTFS fast path: `FSCTL_ENUM_USN_DATA` enumeration → parent FRN reconstruction → sizes/attributes via `OpenFileById`/`GetFileInformationByHandleEx` | On real NTFS volume, enumeration throughput ≥ WizTree-class; fallback detected and reported correctly |
| 3.2 | Windows fallback: when MFT unavailable, explain limitation, mark method=Partial/Incomplete, continue | Fallback path triggered and logged; scan still reports accurate provenance |
| 3.3 | Linux optimization: statx-based enumeration, permission checks, trash integration | Linux core features functional where supported |
| 3.4 | Compact data structures: arena Vec<Node>, u32 ids, boxed small strings, no object graph | Memory profiling shows linear growth with entry count; peak RAM < target threshold |
| 3.5 | Incremental aggregation: directory totals computed as nodes arrive; finalized on close | No second full pass after filter/workspace change |
| 3.6 | Benchmark suite: scan startup, enumeration throughput, files/dirs/s/sec, metadata processing, peak RAM, CPU, UI responsiveness, treemap/radial gen, rendering, search latency, hash throughput, file op throughput | Measured on Windows + Linux; compared against conventional enumeration; documented hardware context |
| 3.7 | Scanner validation: empty directories, deep trees, many small files, few enormous files, unicode, long paths | Validation script exercises all edge cases; report generated |

**Gate:** Benchmark suite passes on both OSes; NTFS fast path beats conventional enumeration by ≥ X%; memory stays bounded at Y MB for Z million entries.

---

## Phase 4 — Visualization Engine *(Deliverable: Complete interactive visualization subsystem)*

| # | Task | Acceptance Gate |
|---|------|-----------------|
| 4.1 | Treemap: squarified layout, area proportional to storage usage, nested rectangles, extension colors, hover/tooltip, click selection, double-click nav, breadcrumb, search highlighting, min rect size, logical/allocated toggle, smooth transitions, incremental update | Interactive treemap renders in GPUI; zoom/drill-down responsive; no single element per file |
| 4.2 | Radial sunburst: root at center, concentric rings, angular segments, depth, drill-down, hover details, selection sync | Sunburst renders; root matches scanned root dir |
| 4.3 | Additional visualizations: circle packing, icicle, radial partition, heatmap, extension distribution, storage composition dashboard, timeline, duplicate viz, directory size dist, file type analysis | Each implemented behind shared `visualization` trait |
| 4.4 | Synchronized panes: treemap+file tree, treemap+radial, compare, resize, hide, reorder, save/restore layouts, sync selections/filters/navigation | Panes share selection and filter state; saved layouts persist across sessions |
| 4.5 | Saved workspaces: disk overview, deep analysis, duplicate analysis, file type analysis, developer storage analysis, custom layouts | User can create/load saved workspaces; export/import JSON |
| 4.6 | Layout math: squarified treemap first, then radial; stable across resizes | Layout algorithm deterministic; no flickering during resize |

**Gate:** Two synchronized panes render simultaneously; saving/loading workspace persists; layout survives window resize; no UI freeze during heavy computation.

---

## Phase 5 — File Manager *(Deliverable: Fully functional file manager)*

| # | Task | Acceptance Gate |
|---|------|-----------------|
| 5.1 | Dynamic file tree: lazy loading, virtualized rendering, expand/collapse, breadcrumbs, file icons, sizes, allocated sizes, dates, extensions, sort, multi-selection, keyboard nav, drag & drop, search, filter, hidden files, dir totals, nav history, favorites, bookmarks, recent locations, refresh | Tree view handles 50k+ entries smoothly; keyboard navigation works |
| 5.2 | File operations: open, copy/move/rename/delete/permanent delete/recycle bin/restore/create dir/file/duplicate/select-all/invert/properties/change attrs/shortcuts/drag-drop/multi-file | All ops succeed or fail with safe cancellation; collisions offer skip/overwrite/rename/keep-both |
| 5.3 | Safety contract: confirm destructive ops, display progress, safe cancellation, partial failures, locked files, permissions, collisions, skip/overwrite/rename/keep-both policies, preserve timestamps/metadata, safe staging | Every operation has preview + confirmation prompt before destructive action |
| 5.4 | Native OS integrations: Recycle Bin, Trash (Linux), Explorer context menus, shell associations, file associations | Verified on Windows and Linux |

**Gate:** File tree navigates 10k+ entries without lag; file ops preserve source paths and metadata; recycle bin visible on Windows.

---

## Phase 6 — Advanced File Utilities *(Deliverable: Advanced filesystem utility suite)*

| # | Task | Acceptance Gate |
|---|------|-----------------|
| 6.1 | Hashing: CRC32, MD5, SHA-1/256, BLAKE3, xxHash — single-file, batch, directory, comparison, manifest import/export, checksum verification, previews | All algorithms available; manifests support import/export |
| 6.2 | Duplicate detection: staged approach (group by size → unique candidates → sample hashes → full hashes → verify equality); never auto-delete | Duplicates grouped; false positives rejected; duplicates visualization present |
| 6.3 | Search: filename, extension, path, directory, size ranges, date filters, attribute filters, hash search, file-type filters, regex, saved searches, quick search, incremental results, virtualized lists | Query builder supports combined filters; saved searches persist |
| 6.4 | Archive management: ZIP, 7z, RAR, TAR family, extract here/to/dir/selected/files, add files, integrity test, browse, compression ratios, password-protected, multi-volume | All archive formats handled; integrity verified post-extract |
| 6.5 | Developer storage analysis: Git repos, Cargo targets, Rust artifacts, node_modules, caches (npm/pnpm/yarn/python/gradle/maven/cmake/VS/Docker/VMs), total by category, largest repos/artifacts, cleanup candidates, historical growth | Report includes all categories; no automatic deletions |

**Gate:** Hash coverage complete; duplicate detection finds ≥ 95% of known dupes; search returns correct rows for all query types; archive extraction validates integrity.

---

## Phase 7 — Persistence and Automation *(Deliverable: Complete analysis and automation ecosystem)*

| # | Task | Acceptance Gate |
|---|------|-----------------|
| 7.1 | Saved scans: SQLite per-scan, WAL mode, transactional publication, schema migrations, interruption-safe | Interrupted save restores prior scans intact |
| 7.2 | Scan comparison: diff between two scans, highlight changes (size, timestamp, hash) | Comparison highlights added/deleted/modified files |
| 7.3 | History: track directory growth, new/removed/deleted files, modified files | Growth tracking enabled; new files flagged |
| 7.4 | Import/export: JSON and CSV exports; import only app-produced databases | Untrusted SQLite import blocked with clear message |
| 7.5 | CLI: headless scan, export results, find large files, calculate hashes, find duplicates, compare scans, run profiles | CLI uses same engine as desktop; commands match docs |
| 7.6 | Configuration profiles: multiple config sets, env var override, hot-reload | Switching profiles reloads without restart |
| 7.7 | Workspace persistence: save/restore layouts, sync selections/filters/navigation | Restored layout identical to saved version |
| 7.8 | Automated changelogs: git-cliff, cargo-release, tags, release notes, artifact publishing | CI generates changelog on push; tag created automatically |

**Gate:** All persisted data survives crash/interrupt; CLI mirrors desktop behavior exactly; config reload works.

---

## Phase 8 — Product Polish *(Deliverable: Release candidate)*

| # | Task | Acceptance Gate |
|---|------|-----------------|
| 8.1 | UI refinement: dark/light/system themes, high contrast, accessibility (keyboard-only nav, focus indicators, screen reader, text scaling, reduced motion) | All themes load; WCAG basics met; no broken keyboard shortcuts |
| 8.2 | Accessibility: screen reader support, focus management, ARIA equivalents via GPUI components | Screen reader reads file names, dirs, stats |
| 8.3 | Performance profiling: scan startup, enumeration throughput, peak RAM/CPU, UI responsiveness, treemap/radial gen, rendering, search latency | Profiling confirms no unbounded growth; meets Phase 3 thresholds |
| 8.4 | Cross-platform validation: Windows + Linux builds, both default and Wayland-only Linux | Both platforms produce installable binary/AppImage/portable |
| 8.5 | Installer packaging: portable ZIP, MSIX (Windows), AppImage/Linux Debian/RPM/Portable binary | Packaging scripts generate all four formats |
| 8.6 | Regression testing: full suite (scanner, viz, file ops, reliability) | Zero flaky tests; coverage ≥ 80% |
| 8.7 | Documentation: README, CHANGELOG, SECURITY, BENCHMARKS, BUILDING, RELEASE, CONTRIBUTING | Docs updated alongside code |

**Gate:** Release candidate passes all regression suites; builds on both OSes; documentation complete; security review cleared.

---

## Phase 9 — Stable Release *(Deliverable: Production-ready 1.0.0)*

| # | Task | Acceptance Gate |
|---|------|-----------------|
| 9.1 | Full test validation: scanner, visualization, file ops, reliability | All tests green; no false completions |
| 9.2 | Security review: input sanitization, path traversal, privilege escalation, info leakage | Review sign-off |
| 9.3 | Performance review: measured against Phase 3 baselines | Within spec, reproducible |
| 9.4 | Windows release: port, installer, MSIX, architecture-specific builds | Installs cleanly on Windows 10/11 |
| 9.5 | Linux release: AppImage, Debian package, RPM package, portable binary | All four formats verified |
| 9.6 | Version 1.0.0 preparation: SemVer bump, automated changelog, tagged release | `git tag v1.0.0`; `cargo release --version 1.0.0` |
| 9.7 | Artifact publishing: SHA-256 checksums, optional code signing | Checksums generated; signing status noted |
| 9.8 | Post-launch monitoring: scan history trends, storage consumption alerts | Dashboards show expected patterns |

**Gate:** 1.0.0 ready for production deployment; monitoring dashboards active; release artifacts published.

---

## Milestone summary table

| Phase | Deliverable | Key Gate |
|-------|-------------|----------|
| 0 | Architecture approved | Research items 0.4–0.6 done; specs written |
| 1 | Shell built, CI passing | Baseline benchmark recorded |
| 2 | Cross-platform scanner working | Tests cover all edge cases |
| 3 | Production scanning engine | Benchmark suite passes on both OSes |
| 4 | Interactive visualization subsystem | Two synchronized panes, saved workspaces |
| 5 | File manager functional | 10k+ entries, safety contracts met |
| 6 | Advanced utilities (hash, dup, search, archive, storage insights) | Coverage ≥ 95%, integrity verified |
| 7 | Persistence + automation (SQLite WAL, CLI, config, history) | Crash-safe saves, CLI parity |
| 8 | Product polish (accessibility, perf, cross-platform, packaging, docs) | Regression green, docs complete |
| 9 | **1.0.0** | All gates passed; artifacts published |

---

## Non-goals (explicitly NOT implemented)

- GPU-friendly treemap rendering (investigated, deferred to later iteration).
- Binary partitioning treemaps (research stage).
- Slice-and-dice treemap variants (research stage).
- Icicle chart animation (research stage).
- Reduced-motion mode (design consideration, not shipped yet).

## Outstanding tasks

- [ ] Run Phase 1 CI on both Windows and Linux; fix any warnings.
- [ ] Write and execute benchmark suite (Phase 3) on actual hardware.
- [ ] Implement NTFS fast path and validate on a real NTFS volume.
- [ ] Wire up Recycle Bin integration (Windows only).
- [ ] Add command palette (Ctrl+Shift+P) shortcut mapping.
- [ ] Configure truss integration on Linux.
- [ ] Draft security review checklist and begin.
- [ ] Schedule regression test execution cadence.

## Next milestone

After Phase 1 gate: move to Phase 2 — implement the generic cross-platform scanner and run the first test suite. Target completion: **Week 3–4**.