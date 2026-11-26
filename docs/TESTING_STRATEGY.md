# TESTING STRATEGY — TimberStat

Rule (Plan §20): test filesystem edge cases; never claim a test passed unless it was run.
A test report must separate **executed**, **failed**, and **not run**.

## 1. Test levels

| Level | Tooling | Scope |
|---|---|---|
| Unit | `cargo test` | config, error mapping, size arithmetic, path normalisation, layout math, tree invariants |
| Property | `proptest` (Phase 6) | path handling, duplicate grouping, diff/merge invariants; commit `proptest-regressions/` |
| Integration | `cargo test --test` with `tempfile` | scanner against generated fixture trees, persistence round-trips |
| UI smoke | `gpui-kit` `test-support` (`#[gpui_kit::test]`) | window opens, theme applies, controls respond; only once headless works on CI |
| Differential | scanner comparison (Phase 3) | NTFS fast path vs generic traversal on the same fixture |
| Fuzz | `cargo-fuzz` (Phase 3 if a parser is added) | malformed or truncated USN/MFT records |
| Manual | checklist in `docs/` | real drives, network shares, removable media, cloud placeholders |

## 2. Filesystem fixture matrix

### Common (both platforms)
empty directories; deep nesting; Unicode and non-UTF-8 names (Linux bytes); long paths; permission denied;
hard links; symlink loops; broken symlinks; sparse files; locked/in-use files; files changing during scan;
deleted during scan; interrupted scan (cancel); zero-byte and tiny files; very large files.

### Windows only
reserved names (`CON`, `NUL`, `COM1`…); trailing dots/spaces; UNC paths; `\\?\` long paths; junctions and
symlinks (may need privilege — separate lane); alternate data streams; reparse points; cloud placeholders;
ACL denial; case-preserving behaviour; NTFS vs ReFS vs FAT32 vs exFAT volumes (manual / VHD-backed where possible).

### Linux only
mount boundaries and bind mounts; inode reuse after unlink; FIFOs, sockets, device nodes (never opened);
open-but-unlinked files; tmpfs and overlayfs; xattrs; root vs non-root permission bypass (separate lane).

## 3. Invariants that must always hold

- Cancelled scan is never reported as complete.
- Interrupted save never corrupts a previously saved scan.
- Hard links: physical total counts each `(volume, file id)` once; logical total counts each path.
- Treemap: child rectangles lie inside parent; areas proportional to size; no overlap; zero-byte and tiny items handled.
- View, filter or workspace change never triggers a scan (assert via scan counter in tests).
- No destructive operation without explicit confirmation (tested by attempting it without a confirmation token).
- Unknown config version and corrupt config files are rejected with a typed error and a safe default.

## 4. Phase 1 test scope (what is actually built)

- Config: default values, serde round-trip, unknown field tolerance, unknown version rejected, corrupt file rejected,
  atomic write leaves the old file intact if the temp write fails.
- Errors: each typed error renders a message; anyhow boundary conversion works.
- Logging: subscriber initialises once and rejects a second initialisation without panicking.
- UI smoke: only if headless rendering works on the CI runner. Otherwise recorded as **not run**.

## 5. CI lanes

| Lane | Runs on | Default |
|---|---|---|
| fmt, clippy `-D warnings`, test `--locked` | Windows + Linux | every push / PR |
| dependency policy (`cargo-deny`, `cargo-audit`) | Linux | every push / PR |
| NTFS fast path, reparse, root-only tests | Windows elevated / Linux root | opt-in, scheduled |
| benchmarks | shared runner | informational |
| network / removable media | — | manual only |

## 6. Reporting

Every milestone report lists: tests **executed** with counts, **failed**, and **not run** with the reason.
No "passing" claim is made for a test that was skipped or not run.
