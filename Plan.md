# MASTER PROJECT PLAN

## Next-Generation Disk Analyzer \& File Management Suite

**Project status:** Approved product vision\
**Primary language:** Rust\
**UI framework:** GPUI\
**Component library:** GPUI Kit\
**Primary platform:** Windows 10/11\
**Secondary platform:** Linux\
**Versioning:** Semantic Versioning\
**Release automation:** GitHub Actions + Conventional Commits + git-cliff

---

# 1. Executive Summary

Build an exceptionally fast, memory-efficient, modern native desktop application combining disk analysis, filesystem visualization, file management, storage intelligence, and advanced file utilities.

The application draws inspiration from:

- WizTree: [https://diskanalyzer.com/](https://diskanalyzer.com/)
- MacDirStat: <https://github.com/MichaelStromberg/macdirstat>
- SquirrelDisk: [https://github.com/adileo/squirreldisk](https://github.com/adileo/squirreldisk)

It should go significantly beyond these applications by combining their strongest features with a comprehensive file manager, advanced storage analysis, multiple visualization engines, and professional developer tooling.

The application must be:

- Extremely fast.
- Memory efficient.
- Responsive with millions of filesystem entries.
- Designed around Windows capabilities.
- Fully functional on Linux where platform capabilities permit.
- Visually polished.
- Modular and maintainable.
- Extensible.
- Fully tested.
- Automatically versioned and released.

## Core product philosophy

**Performance without compromising correctness, usability, or reliability.**

The scanning engine, filesystem model, visualization engine, and UI must be independently designed and optimized.

The application must never require another filesystem scan simply because a user changes visualization, applies a filter, or switches workspace layouts.

---

# 2. Approved Technology Stack

## Core

| **Technology** | **Purpose** |
| --- | --- |
| Rust | Application and backend |
| GPUI | Native desktop interface |
| GPUI Kit | UI component foundation |
| Cargo | Build system |
| Cargo workspaces | Modular architecture |
| Tokio | Appropriate asynchronous operations |
| Rayon | Bounded CPU-parallel workloads where appropriate |
| Serde | Serialization |
| tracing | Structured logging |
| thiserror | Domain errors |
| anyhow | Application-level error propagation |
| Criterion | Benchmarking |
| Proptest | Property-based testing |
| GitHub Actions | CI/CD |

Research and verify compatibility between GPUI, GPUI Kit, Rust versions, and their dependencies before implementation.

Do not introduce unnecessary frameworks or dependencies.

## Platform-specific technologies

### Windows

Investigate and use appropriate Windows APIs for:

- NTFS MFT enumeration.
- Volume information.
- File identifiers.
- File attributes.
- Reparse points.
- Hard links.
- Recycle Bin integration.
- Windows Shell integration.
- Native file operations.
- UAC elevation.
- Archive associations.
- Explorer integration.
- Filesystem metadata.

### Linux

Use appropriate Linux facilities for:

- Filesystem enumeration.
- File metadata.
- Permissions.
- Symbolic links.
- Trash integration.
- Mount discovery.
- Filesystem-specific optimizations.
- Native file operations.

Maintain platform-specific functionality behind dedicated abstractions.

---

# 3. Architecture

Use a modular Cargo workspace.

Suggested structure:

text

```
project/
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml
├── README.md
├── LICENSE
├── CHANGELOG.md
├── ARCHITECTURE.md
├── CONTRIBUTING.md
├── SECURITY.md
├── ROADMAP.md
├── BENCHMARKS.md
├── BUILDING.md
├── RELEASE.md
│
├── crates/
│   ├── app/
│   ├── ui/
│   ├── ui-components/
│   ├── core/
│   ├── filesystem/
│   ├── scanner/
│   ├── scanner-ntfs/
│   ├── scanner-generic/
│   ├── visualization/
│   ├── visualization-treemap/
│   ├── visualization-radial/
│   ├── visualization-advanced/
│   ├── file-tree/
│   ├── file-operations/
│   ├── hashing/
│   ├── duplicates/
│   ├── archive/
│   ├── search/
│   ├── storage-insights/
│   ├── scan-history/
│   ├── scan-comparison/
│   ├── database/
│   ├── cli/
│   ├── platform/
│   ├── platform-windows/
│   ├── platform-linux/
│   └── configuration/
│
├── assets/
├── docs/
├── benchmarks/
├── tests/
├── scripts/
└── .github/
    └── workflows/
```

This is a proposed structure, not a requirement to create empty crates prematurely. Establish module boundaries based on actual implementation needs.

## Architectural requirements

- UI must not perform blocking filesystem operations.
- Scanner must not depend on GPUI.
- Visualization calculations must not depend on GPUI.
- File operations must be independent of visualization.
- Platform-specific APIs must not leak into shared business logic.
- Every expensive operation must support progress reporting and appropriate cancellation.
- Concurrency must be bounded.
- Large datasets must use efficient data structures.
- The application must support incremental updates.
- UI rendering must use virtualization or custom rendering where appropriate.

---

# 4. Scanning Engine

This is the highest-priority subsystem.

## 4.1 Windows NTFS MFT scanner

Implement a specialized NTFS scanner that investigates direct MFT enumeration to achieve WizTree-class performance.

Capabilities:

- Enumerate filesystem records.
- Read file names.
- Reconstruct directory hierarchies.
- Identify parent relationships.
- Retrieve file sizes.
- Retrieve allocated sizes where available.
- Retrieve timestamps.
- Retrieve relevant attributes.
- Identify hard links.
- Handle deleted or stale records.
- Handle inaccessible records.
- Handle reparse points.
- Handle fragmented metadata.
- Handle changing filesystems.
- Detect incomplete enumeration.
- Report errors accurately.

Use read-only filesystem access.

Investigate safe, documented Windows APIs and privilege requirements. Do not assume raw volume access is available or safe in every environment.

### Fallback behavior

If MFT scanning is unavailable:

1. Detect the limitation.
2. Explain why the preferred scanner cannot run.
3. Fall back to conventional filesystem traversal.
4. Continue reporting accurate progress.
5. Mark the scan method in scan metadata.

## 4.2 Generic filesystem scanner

Support:

- NTFS.
- ReFS where possible.
- FAT32.
- exFAT.
- ext4.
- Btrfs.
- XFS.
- Other filesystems through generic traversal.

Support:

- Local drives.
- Removable drives.
- Mounted volumes.
- Network paths where supported.
- Individual directories.
- Multiple directories.
- Entire filesystems.

## 4.3 Scanner functionality

- Scan.
- Rescan.
- Refresh selected subtree.
- Cancel.
- Pause/resume where technically supported.
- Exclude paths.
- Exclude file types.
- Live progress.
- Scan history.
- Saved scans.
- Scan statistics.
- Error reporting.
- Export results.

## 4.4 Storage accuracy

Distinguish:

- Logical size.
- Allocated size.
- Size on disk.
- Apparent size.
- Physical storage consumption where measurable.

Correctly account for:

- Hard links.
- Sparse files.
- Compression.
- Symbolic links.
- Junctions.
- Reparse points.
- Cloud placeholders.
- Filesystem metadata.

Document unavoidable differences between logical accounting and actual physical storage consumption.

## 4.5 Performance engineering

Implement:

- Bounded parallel enumeration.
- Efficient metadata retrieval.
- Compact file records.
- Efficient parent-child indexing.
- Incremental directory aggregation.
- Memory-efficient storage.
- Batched UI updates.
- Backpressure.
- Cancellation.
- Scan prioritization where useful.
- Allocation profiling.
- Memory profiling.

Never load file contents during ordinary scans.

---

# 5. Unified Filesystem Data Model

Create one authoritative filesystem representation used by:

- Treemap.
- Radial visualization.
- File tree.
- Search.
- Duplicate detection.
- Storage insights.
- Scan comparison.
- Exports.

Avoid maintaining separate conflicting filesystem representations for each feature.

The model should support:

- Stable internal identifiers.
- Parent identifiers.
- Child indexing.
- File metadata.
- Directory metadata.
- Logical sizes.
- Allocated sizes.
- File types.
- Timestamps.
- Attributes.
- Platform-specific metadata.
- Hard-link relationships.
- Scan provenance.

Use compact representations and indexed storage where appropriate.

Support millions of records without requiring a heavyweight object graph.

---

# 6. Visualization Engine

Create an extensible visualization system.

All visualizations must share:

- Selection.
- Navigation.
- Search.
- Filtering.
- Color configuration.
- Metadata tooltips.
- Directory hierarchy.
- Zoom.
- Breadcrumbs.
- Refresh.
- Export capabilities where appropriate.

Changing visualization must never initiate another filesystem scan.

## 6.1 Treemap

\<REDACTED\_PLACEHOLDER\> \</REDACTED\_PLACEHOLDER\>

Implement a highly interactive WizTree-inspired treemap.

Requirements:

- Nested rectangles.
- Area proportional to storage usage.
- Directory hierarchy.
- File-level representation.
- Extension-based colors.
- Directory-based colors.
- Custom color palettes.
- Hover highlighting.
- Tooltips.
- Click selection.
- Double-click navigation.
- Right-click context menu.
- Zoom and drill-down.
- Breadcrumb navigation.
- Search highlighting.
- Label visibility thresholds.
- Minimum rectangle sizes.
- Logical/allocated size toggle.
- Smooth transitions.
- Incremental updates.

Investigate:

- Squarified treemaps.
- Slice-and-dice treemaps.
- Binary partitioning.
- Stable layouts.

Use efficient custom rendering rather than creating one heavyweight GPUI element per rectangle.

## 6.2 Radial sunburst

Implement a complete radial visualization.

Requirements:

- Root directory at the center.
- Concentric hierarchy rings.
- Proportional angular segments.
- Directory depth.
- Interactive drill-down.
- Zoom.
- Hover details.
- Selection synchronization.
- File-type coloring.
- Configurable depth.
- Animated navigation.
- Search highlighting.

## 6.3 Additional visualizations

All approved:

1. Circle packing.
2. Icicle chart.
3. Radial partition.
4. Squarified treemap.
5. Slice-and-dice treemap.
6. Binary treemap.
7. Disk usage heatmap.
8. File extension distribution.
9. Storage composition dashboard.
10. File modification timeline.
11. Duplicate file visualization.
12. Directory size distribution.

## 6.4 Multiple synchronized visualization panes

Provide a visualization workspace.

Users can:

- Display treemap and file tree simultaneously.
- Display treemap and radial visualization together.
- Compare visualization types.
- Resize panels.
- Hide panels.
- Reorder panels.
- Save layouts.
- Restore previous layouts.
- Synchronize selections.
- Synchronize filters.
- Synchronize navigation.

## 6.5 Visualization workspace manager

Implement saved workspaces:

- Disk overview.
- Deep directory analysis.
- Duplicate analysis.
- File type analysis.
- Developer storage analysis.
- Custom layouts.

Allow users to create and save their own workspaces.

---

# 7. Complete File Manager

The application must include a fully functional file manager.

## File tree

- Lazy loading.
- Virtualized rendering.
- Expand/collapse.
- Breadcrumbs.
- File icons.
- File sizes.
- Allocated sizes.
- Modification dates.
- File extensions.
- Sorting.
- Multi-selection.
- Keyboard navigation.
- Drag and drop.
- Search.
- Filtering.
- Hidden files.
- Directory totals.
- Navigation history.
- Favorites.
- Bookmarks.
- Recent locations.
- Refresh.

## File operations

Implement:

- Open.
- Open with.
- Copy.
- Cut.
- Paste.
- Move.
- Rename.
- Delete.
- Permanent delete.
- Recycle Bin.
- Restore.
- Create directory.
- Create file.
- Duplicate.
- Select all.
- Invert selection.
- Copy full path.
- Copy relative path.
- Copy name.
- Copy metadata.
- Properties.
- Change attributes where supported.
- Create shortcuts where supported.
- Drag and drop.
- Multi-file operations.

## File operation safety

- Confirm destructive operations.
- Display progress.
- Support safe cancellation.
- Report partial failures.
- Handle locked files.
- Handle permissions.
- Handle collisions.
- Support skip, overwrite, rename, and keep-both policies.
- Never silently overwrite.
- Preserve timestamps and metadata when requested and supported.
- Use safe staging where appropriate.

## File properties

Show:

- Filename.
- Full path.
- File size.
- Allocated size.
- Type.
- Created time.
- Modified time.
- Accessed time.
- Attributes.
- Permissions.
- Owner where available.
- Hashes.
- File identifiers.
- Hard-link details.
- Associated application.

---

# 8. Hashing and Duplicate Detection

Support:

- CRC32.
- MD5.
- SHA-1.
- SHA-256.
- SHA-512.
- BLAKE3.
- xxHash.

Clearly identify cryptographic and non-cryptographic algorithms.

Features:

- Single-file hashing.
- Batch hashing.
- Directory hashing.
- File comparison.
- Checksum verification.
- Checksum manifests.
- Manifest import/export.
- Duplicate detection.
- Duplicate groups.
- Duplicate directory analysis.
- Reclaimable space calculation.
- Preview before deletion.

## Optimized duplicate detection

Use a staged approach:

1. Group files by size.
2. Remove unique-size candidates.
3. Optionally compare inexpensive file samples.
4. Calculate complete hashes.
5. Verify file equality before offering destructive actions.

Include a dedicated duplicate visualization and duplicate management workspace.

Never automatically delete duplicates.

---

# 9. Archive Management

Support:

- ZIP.
- 7z.
- RAR.
- TAR.
- TAR.GZ.
- TAR.XZ.
- TAR.ZST.
- GZIP.
- BZIP2.
- XZ.
- Zstandard.

## Windows applications

Detect and integrate:

- NanaZip.
- 7-Zip.
- WinRAR.
- WinZip.
- Windows built-in archive functionality.

Allow configurable preferred handlers.

## Archive features

- Create archive.
- Extract here.
- Extract to directory.
- Extract selected files.
- Add files.
- Test integrity.
- Browse archive contents.
- Virtual archive filesystem.
- Compression ratios.
- Compressed versus original size.
- Password-protected archives where supported.
- Multi-volume archives where supported.

Do not assume external applications are installed.

---

# 10. Advanced Search and Filtering

Implement:

- Filename search.
- Extension search.
- Path search.
- Directory search.
- Size search.
- Size ranges.
- Date filters.
- Attribute filters.
- Hash search.
- File type filters.
- Duplicate filters.
- Regular expressions.

Example queries:

- Files larger than 1 GB.
- Files modified more than one year ago.
- Largest 100 files.
- All ISO files.
- Empty directories.
- Files without extensions.
- Duplicate files.
- Files above a configurable threshold.
- Largest directories.
- Files consuming the most allocated space.

Features:

- Combined filters.
- Saved searches.
- Search history.
- Quick search.
- Advanced query builder.
- Incremental results.
- Virtualized result lists.

---

# 11. Storage Intelligence

Implement an intelligent storage analysis subsystem.

Identify:

- Temporary files.
- Application caches.
- Browser caches.
- Log files.
- Old downloads.
- Large installers.
- Old archives.
- Crash dumps.
- Empty directories.
- Duplicate files.
- Development caches.
- Build artifacts.
- Virtual machine images.
- Large media collections.
- Game installations.
- Docker-related storage where accessible.

## Cleanup recommendations

For every recommendation:

- Explain what the item is.
- Explain why it was identified.
- Show its size.
- Show potential reclaimable space.
- Explain possible risks.
- Require explicit confirmation.

Never automatically delete files.

Do not classify files as safe to remove merely because they are old or large.

---

# 12. Developer Storage Analysis

Create a dedicated developer-oriented analysis workspace.

Detect and analyze:

- Git repositories.
- Cargo target directories.
- Rust build artifacts.
- node\_modules.
- npm and pnpm caches.
- Yarn caches.
- Python virtual environments.
- Python package caches.
- Gradle caches.
- Maven caches.
- CMake build directories.
- Visual Studio build artifacts.
- Docker-related storage.
- Virtual machine images.

Show:

- Total storage by category.
- Largest repositories.
- Largest build artifacts.
- Potential cleanup candidates.
- Directory distribution.
- Historical growth where scan history exists.

Provide actionable insights without automatically deleting developer data.

---

# 13. Command Palette

Implement a global command palette inspired by VS Code.

Shortcut: Ctrl+Shift+P.

Support searchable commands for:

- Start scan.
- Stop scan.
- Refresh.
- Change visualization.
- Switch workspace.
- Open directory.
- Search files.
- Find duplicates.
- Calculate hashes.
- Create archive.
- Change theme.
- Toggle panels.
- Export scan.
- Compare scans.
- Open settings.
- Navigate to recent locations.

Support keyboard navigation and command discovery.

Allow users to configure shortcuts where feasible.

---

# 14. Scan History and Comparison

Implement persistent scan sessions.

Features:

- Save scans.
- Reopen scans.
- Export scans.
- Import scans.
- Compare scans.
- Track directory growth.
- Identify new files.
- Identify deleted files.
- Identify modified files.
- Compare sizes.
- Highlight changes visually.
- Show historical storage trends.

Persist data incrementally and safely.

Use versioned storage schemas and migration support.

An interrupted save must not corrupt previously saved scans.

---

# 15. Main UI Design

The interface should feel like a premium native desktop application.

Design inspiration:

- Windows 11.
- Modern developer tools.
- VS Code.
- Linear.
- Professional filesystem utilities.

Avoid excessive visual decoration.

## Main window

### Top toolbar

- Drive selector.
- Scan controls.
- Search.
- Refresh.
- Visualization selector.
- Workspace selector.
- Settings.

### Left sidebar

- Drives.
- Favorites.
- Bookmarks.
- Scan history.
- Saved searches.
- Workspaces.

### Main workspace

- Visualization panels.
- Dynamic file tree.
- Breadcrumb navigation.
- Search results.
- Resizable split panes.

### Information panel

- Selected item information.
- Directory statistics.
- Scan progress.
- Operation progress.
- Storage metrics.

## Themes

- Dark.
- Light.
- System.
- Custom accent.
- Custom visualization palettes.
- High contrast.

## Interaction

- Keyboard shortcuts.
- Context menus.
- Tooltips.
- Drag and drop.
- Multi-selection.
- Smooth transitions.
- Command palette.
- Responsive feedback.

## Accessibility

- Keyboard-only navigation.
- Focus indicators.
- Screen reader support where possible.
- Text scaling.
- High contrast.
- Reduced motion.

Use GPUI Kit components wherever suitable.

Custom-render performance-critical visualization surfaces where necessary.

---

# 16. Additional Power Features

Implement an extensible foundation for:

- Portable mode.
- CLI.
- Headless scanning.
- JSON export.
- CSV export.
- Configurable exclusions.
- Custom file categories.
- Shell integration.
- Explorer context menu integration.
- Saved layouts.
- Configurable shortcuts.
- Multiple drives.
- Network paths.
- Removable media.
- Custom scan profiles.

## CLI

Provide commands for:

- Scanning.
- Exporting results.
- Finding large files.
- Calculating hashes.
- Finding duplicates.
- Comparing scans.
- Running predefined analysis profiles.

The CLI must use the same underlying scanning and filesystem engine as the desktop application.

---

# 17. Platform Support

## Windows-first features

Prioritize:

- NTFS MFT scanning.
- Windows volume discovery.
- Recycle Bin.
- Native file dialogs.
- Shell integration.
- Explorer context menus.
- UAC.
- File associations.
- Native metadata.
- Archive integrations.

## Linux features

Provide:

- Filesystem enumeration.
- File management.
- Permissions.
- Symbolic links.
- Trash integration where supported.
- Mount discovery.
- Archive management.
- Hashing.
- Duplicate detection.
- Search.
- Visualizations.
- Saved scans.

Clearly communicate unavailable or reduced functionality.

Maintain equivalent core workflows where feasible.

---

# 18. Performance and Benchmarking

Create a dedicated benchmarking suite.

Measure:

- Scan startup.
- Enumeration throughput.
- Files per second.
- Directories per second.
- Metadata processing.
- Peak RAM.
- CPU utilization.
- UI responsiveness.
- Treemap generation.
- Radial generation.
- Rendering performance.
- Search latency.
- Hash throughput.
- File operation throughput.

Test:

- Small directories.
- Hundreds of thousands of files.
- Millions of files.
- Tens of millions of entries.
- Deep directory trees.
- Many small files.
- Few enormous files.

Compare:

- Windows MFT enumeration.
- Conventional Windows enumeration.
- Linux enumeration.

Record:

- Hardware.
- OS.
- Filesystem.
- Privilege level.
- Cache state.
- Dataset characteristics.
- Benchmark methodology.

Do not invent or promise benchmark numbers.

Optimize based on profiling and repeatable measurements.

---

# 19. Semantic Versioning and Release Automation

Use Semantic Versioning:

MAJOR.MINOR.PATCH

Examples:

- 0.1.0 — Initial development.
- 0.2.0 — New functionality.
- 0.2.1 — Bug fixes.
- 1.0.0 — Stable release.

## Conventional Commits

Examples:

- feat: add radial visualization
- fix: correct hard-link accounting
- perf: optimize NTFS scanning
- refactor: improve filesystem indexing
- docs: update architecture
- test: add scanner regression tests
- chore: update dependencies
- build: improve release packaging
- ci: add Linux validation

## Automated changelogs

Use:

- git-cliff.
- cargo-release.
- GitHub Actions.

Automate:

- Version updates.
- Changelog generation.
- Git tagging.
- Release notes.
- Release creation.
- Artifact publishing.
- Version consistency validation.

## CI workflows

Create dedicated workflows for:

1. Pull request validation.
2. Formatting.
3. Clippy.
4. Unit tests.
5. Integration tests.
6. Windows builds.
7. Linux builds.
8. Release builds.
9. Dependency auditing.
10. Release publishing.

## Release artifacts

Windows:

- Portable ZIP.
- Installer.
- Optional MSIX.
- Architecture-specific builds.

Linux:

- AppImage where feasible.
- Debian package.
- RPM package.
- Portable binary.

Generate SHA-256 checksums.

Implement code signing where infrastructure is available.

Never publish a release when required checks fail.

---

# 20. Quality Assurance

## Scanner tests

- Empty directories.
- Deep nesting.
- Large datasets.
- Unicode.
- Long paths.
- Permissions.
- Symbolic links.
- Junctions.
- Hard links.
- Sparse files.
- Reparse points.
- Changing files.
- Deleted files.
- Network paths.
- Removable drives.
- Interrupted scans.

## Visualization tests

- Correct proportional sizing.
- Correct hierarchy.
- Zero-byte files.
- Tiny files.
- Large files.
- Massive datasets.
- Selection synchronization.
- Incremental updates.
- Search.
- Filtering.
- Navigation.

## File operation tests

- Copy.
- Move.
- Rename.
- Delete.
- Collision handling.
- Locked files.
- Permissions.
- Cancellation.
- Partial failures.
- Cross-volume operations.

## Reliability requirements

- No UI freezing.
- No unbounded memory growth.
- No silent data loss.
- No false completion reporting.
- No unconfirmed destructive operations.
- No corrupted saved scans.
- No misleading storage totals.

---

# 21. Documentation

Maintain:

- README.md
- ARCHITECTURE.md
- ROADMAP.md
- CHANGELOG.md
- CONTRIBUTING.md
- SECURITY.md
- BENCHMARKS.md
- BUILDING.md
- WINDOWS.md
- LINUX.md
- RELEASE.md

Keep documentation updated as implementation evolves.

---

# 22. Implementation Roadmap

Implementation must proceed in independently testable milestones.

## Phase 0 — Research and Architecture

- Inspect reference projects.
- Inspect GPUI and GPUI Kit.
- Verify dependencies.
- Research NTFS scanning.
- Research Linux enumeration.
- Evaluate data structures.
- Establish architecture.
- Define performance benchmarks.
- Identify technical risks.
- Create implementation specifications.

**Deliverable:** Approved technical architecture and implementation plan.

## Phase 1 — Project Foundation

- Cargo workspace.
- GPUI integration.
- GPUI Kit integration.
- Main application window.
- Theme system.
- Logging.
- Error handling.
- Configuration.
- Initial CI.
- Formatting and linting.
- Versioning foundation.

**Deliverable:** Buildable native application shell.

## Phase 2 — Filesystem Core

- Platform abstraction.
- Drive enumeration.
- Generic traversal.
- Metadata structures.
- Directory hierarchy.
- Size aggregation.
- Cancellation.
- Progress reporting.
- Scanner tests.

**Deliverable:** Working cross-platform filesystem scanner.

## Phase 3 — Extreme Performance

- NTFS MFT scanner.
- Windows fallback.
- Linux optimization.
- Compact data structures.
- Incremental aggregation.
- Memory profiling.
- Benchmark suite.
- Scanner validation.

**Deliverable:** Production-oriented scanning engine.

## Phase 4 — Visualization Engine

- Treemap.
- Radial sunburst.
- Circle packing.
- Icicle.
- Radial partition.
- Heatmap.
- Extension distribution.
- Storage dashboard.
- Shared selection.
- Shared navigation.
- Multiple synchronized panes.
- Saved workspaces.

**Deliverable:** Complete interactive visualization subsystem.

## Phase 5 — File Manager

- Dynamic file tree.
- Virtualization.
- Navigation.
- Context menus.
- File operations.
- Drag and drop.
- Properties.
- Native OS integrations.

**Deliverable:** Fully functional file manager.

## Phase 6 — Advanced File Utilities

- Hashing.
- Duplicate detection.
- Search.
- Advanced filters.
- Archive management.
- Storage insights.
- Developer storage analysis.

**Deliverable:** Advanced filesystem utility suite.

## Phase 7 — Persistence and Automation

- Saved scans.
- Scan comparison.
- History.
- Import/export.
- CLI.
- Configuration profiles.
- Workspace persistence.

**Deliverable:** Complete analysis and automation ecosystem.

## Phase 8 — Product Polish

- UI refinement.
- Accessibility.
- Performance profiling.
- Cross-platform validation.
- Installer packaging.
- Release automation.
- Documentation.
- Regression testing.

**Deliverable:** Release candidate.

## Phase 9 — Stable Release

- Full test validation.
- Security review.
- Performance review.
- Windows release.
- Linux release.
- Version 1.0.0 preparation.
- Automated changelog.
- Published release artifacts.

**Deliverable:** Production-ready 1.0.0.

---

# 23. Mandatory Engineering Rules

1. Never block the GPUI main thread.
2. Never read entire files during normal scanning.
3. Never create millions of individual UI elements.
4. Never use unbounded concurrency.
5. Never silently ignore filesystem errors.
6. Never double-count hard links in physical storage accounting.
7. Never assume Windows capabilities exist on Linux.
8. Never fake implemented functionality.
9. Never leave nonfunctional placeholder controls.
10. Never invent benchmark results.
11. Never perform destructive operations without appropriate user intent.
12. Never introduce unnecessary dependencies.
13. Never compromise correctness for speed.
14. Never make core scanning depend on optional integrations.
15. Always test filesystem edge cases.
16. Always document platform limitations.
17. Always keep the application buildable.
18. Always profile before optimizing.
19. Always maintain versioned persistent data formats.
20. Always validate release artifacts before publishing.

---

# 24. First Assignment to the Coding Agent

Before implementing the application, inspect and research:

### Reference projects

- [https://diskanalyzer.com/](https://diskanalyzer.com/)
- <https://github.com/MichaelStromberg/macdirstat>
- [https://github.com/adileo/squirreldisk](https://github.com/adileo/squirreldisk)

### UI framework

- [https://www.gpui.rs/](https://www.gpui.rs/)
- [https://gpui-kit.com/](https://gpui-kit.com/)

### Technical research

Investigate:

- Windows NTFS MFT enumeration.
- Windows filesystem APIs.
- Linux filesystem enumeration.
- Efficient tree representations.
- GPU-friendly treemap rendering.
- Radial visualization algorithms.
- Large dataset virtualization.
- Archive libraries.
- Hashing libraries.
- Persistent scan databases.

### Produce the following documents

1. ARCHITECTURE.md
2. TECHNICAL\_RESEARCH.md
3. DEPENDENCY\_SELECTION.md
4. PLATFORM\_COMPATIBILITY.md
5. PERFORMANCE\_PLAN.md
6. TESTING\_STRATEGY.md
7. IMPLEMENTATION\_ROADMAP.md

Then create the initial repository and implement Phase 1.

Do not attempt to implement the entire application in one pass.

Work in small, testable increments.

At the end of every milestone, report:

- Completed functionality.
- Created or modified modules.
- Tests performed.
- Build status.
- Benchmark results.
- Known limitations.
- Outstanding tasks.
- Next milestone.

---

# 25. Definition of Done

The application is not considered complete simply because the UI looks finished.

The finished product must have:

- A high-performance Windows NTFS scanning engine.
- Functional generic filesystem scanning.
- Linux compatibility for core features.
- Accurate filesystem size accounting.
- Interactive treemap.
- Interactive radial visualization.
- Additional visualization types.
- Synchronized visualization panes.
- Saved workspaces.
- Fully functional file tree.
- Real filesystem operations.
- Hashing.
- Duplicate detection.
- Archive integration.
- Advanced search.
- Storage intelligence.
- Developer storage analysis.
- Command palette.
- Scan history.
- Scan comparison.
- Export/import.
- CLI.
- Automated semantic versioning.
- Automated changelogs.
- Automated builds and releases.
- Comprehensive tests.
- Reproducible performance benchmarks.
- Complete documentation.

# Final Product Vision

Create a premium native desktop application that combines the scanning performance ambitions of WizTree, the visualization capabilities of modern disk analyzers, the usability of a capable file manager, and the extensibility of a professional developer tool.

It should be fast enough to make analyzing enormous filesystems feel immediate, intelligent enough to help users understand where their storage goes, and powerful enough to manage their files without needing another application.

**Performance is the defining feature. Correctness is non-negotiable. Usability is essential. Extensibility is architectural.**

Build it properly, measure everything, and never sacrifice the foundation for shortcuts.
