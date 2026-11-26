# Changelog

All notable changes to TimberStat are documented here. This file is generated from
Conventional Commits by git-cliff on each release; do not edit it by hand.

## [0.1.0] - 2026-11-26

### Added

- Add GPUI shell and versioned configuration (Phase 1) (aa20e9f)
- Add filesystem model, scanner, and volume scanning in the app (6a24f1c)
- Add NTFS MFT reader (timberstat-ntfs) and name unreadable paths (7d8af64)
- Fast NTFS scan in the window, redesigned layout (665b444)
- Walk progress and partial-result alert, using GPUI Kit components (0556406)
- Fast scan progress bar, and open the window at normal size (510def6)
- WizTree-style nested treemap, pie tooltip sizes, sunburst module split (9971fcc)
- Fast NTFS scan returns the full folder tree (7e35ea5)
- Treemap double-click, tile menu, on-disk size toggle (5dbed38)
- Search highlighting across treemap and sunburst (94bd11c)
- Search highlighting in the folder tree (3f1b74a)
- WizTree-style main layout (2dab704)
- **app:** Widget layer for the content area (1697034)
- **search:** Hmstd-search query language (d7fab28)
- **app:** Query language wired into search, tree and graphs (87cf0be)
- **app:** Settings panel with UI scale, shared-edge widget resize, 2px gap (9f4458e)
- **app:** Tiled widget layout, shadcn-style empty states (ab65a54)
- **fileops:** Delete, copy, move and rename engine with confirmation gate (ec9bb07)
- **app:** Add guarded delete workflow (ef1eb7d)
- **app:** New top bar; Extensions and Summary scroll (f80fbbf)
- **app:** Copy, move and rename dialogs wired into the context menu (1abcb22)
- **duplicates:** Staged duplicate detection and duplicate panel (7838fe6)
- **search:** Dup:yes / dup:no term over the duplicate result (b1a3123)
- [**breaking**] Rename the app to TimberStat (84fbc2c)
- **app:** App icon in the exe, window, taskbar, installers and Linux packages (b9f0f72)
- **app:** Save and restore visualization workspaces (27816a7)

### Fixed

- Symlinks add no logical size; hard-link test accepts either path (131aaf5)
- File tree search lists only matches (d006f97)
- Split the stacked banner (2c1ec1c)
- **app:** Folder tree rows, graph scroll, ui_scale setting (5a3fae4)
- **app:** Widget move pushes neighbours, saved layouts settle, tree rows (8125565)
- **app:** Folder tree rows, graphs fit their widget, 5px padding, compact Summary (1be8a83)
- **app:** Folder tree columns line up; widgets size from measured box; more padding (d27dc28)
- **app:** Set scrollbar mode through the theme API; log theme mode changes (25d7a60)
- **app:** Search finds matches at any depth in the tree and graphs (ef0fec1)
- **fileops:** Reparse-point check compiles cleanly on Linux (6a9c0c4)

### Performance

- List directories on worker threads, cut per-entry cost (34cbeba)

### Documentation

- Add Phase 0 architecture, research, and roadmap (0c8325a)
- New README with hero, C: screenshot, downloads, features and search guide (c982632)
- GitHub social preview image (640x320 and 1280x640 PNG) (f3946ec)

### Tests

- **search:** Refine query integration coverage (ac495e0)
- **duplicates:** Cover the byte-verify gate directly (cb67e56)
- **fileops:** Split the Unix link test into copy and same-volume move (1ecb4c2)

### Build and CI

- Install fontconfig and GPUI system libraries on Ubuntu jobs (b417561)
- Semantic releases with Windows installers and Linux packages (6ee257a)

### Maintenance

- Add CI workflow, dependency policy, README and CHANGELOG (9be01c0)


