# Security Review Record

- **Date:** 2026-11-27
- **Task:** t_26f5adc2 (Plan 21 documentation, Plan 22 Phase 9 "Security review")
- **Scope:** elevated Fast-scan helper trust model, `timberstat-diskprobe` `unsafe` scope,
  `timberstat-archive` handlers.
- **Code state reviewed:** `main` at `174428f`, plus the uncommitted working-tree changes present at
  review time: `crates/engine/src/fast.rs`, `crates/diskprobe/src/lib.rs`,
  `crates/diskprobe/src/file_facts.rs`, `crates/diskprobe/Cargo.toml`, `crates/fileops/src/lib.rs`.
  Line numbers below refer to the working-tree files. If those changes are committed differently,
  re-check the line numbers.

## Method

- **Static, read-only review** of the files listed above. Every claim in this record was checked
  against source with `grep` or by reading the code path. The audit did not trust the earlier
  static-audit summary.
- **Build and test** of `timberstat-archive` and `timberstat-diskprobe`, run on a
  `git archive HEAD` copy in the scratch area (not in the working tree).
  - `cargo test -p timberstat-archive -p timberstat-diskprobe`: archive 5 passed, 0 failed (format
    tests only); diskprobe 1 passed, 1 ignored (`live_probe::time_real_drives`, not run).
- **Not run:**
  - End-to-end Fast scan. It needs a UAC consent prompt, which is a desktop action.
  - Junction or symlink race tests. They need elevation or developer mode.
  - Malicious-archive extraction tests. No handler is wired into the app.
  - `timberstat-engine` tests (`fast_tests.rs` exists only in the working tree).
  Findings marked "needs test" are static conclusions and have not been reproduced.
- **Constraints kept:** no desktop control, no GUI launch, no elevated process started, no writes to
  user data or drives outside the scratch copy.

## Summary

| Severity | Count | IDs |
|---|---|---|
| High | 1 | F-01 |
| Medium | 4 | F-02, F-03, F-04, F-06 |
| Medium (latent, archive not wired) | 1 | F-07 |
| Low | 5 | F-05, F-08, F-09, F-10, F-11 |
| Hygiene / info | 2 | F-12, F-13 |

All findings are **open**. This task documents them and does not fix them. Plan section 22
(Phase 9) gates the 1.0.0 release on a security review, so the open items must be resolved or
explicitly accepted before that release.

## Verified positives

- **No network code** in the crates reviewed.
- **No shell strings.** Archive tools run through argv (`runner.rs::run`, `Command::args`).
- **Helper accepts drive volumes only.** `valid_volume` (fast.rs:352) requires `//./X:`.
- **Helper never reads file contents.** `timberstat-ntfs` has zero `unsafe` and reads only the boot
  sector and MFT records (`summary.rs:46-50`, `reader.rs:77-110`).
- **Diskprobe is read-only.** Volume handles use zero access or `FILE_READ_ATTRIBUTES`; the only
  device control is `IOCTL_STORAGE_QUERY_PROPERTY`.
- **`unsafe` is confined to diskprobe.** `unsafe_code = "forbid"` in `[workspace.lints.rust]`
  (`Cargo.toml:32`); diskprobe overrides it to `"allow"` (`crates/diskprobe/Cargo.toml:19`).
- **`fileops` has no `unsafe`.** The only match is its doc comment (`fileops/src/lib.rs:8`).
- **Helper result parsing is bounded in the common case** and checks the root record, the parent
  index and the magic header (`tree_io.rs:92-125`).

## Findings

### F-01 — High — One UAC consent grants same-user elevated volume listings for the helper's lifetime

- **Where:** `fast.rs:316-337` (`service_main` loop), `fast.rs:341-350` (`take_request`),
  `fast.rs:352-358` (`valid_volume`), `fast.rs:360-365` (`heartbeat_age`), `fast.rs:193-203`
  (`start_heartbeat` writes `alive`).
- **Evidence:** the helper trusts three plain files in a user-writable folder. It does not check who
  wrote them. A request names any drive letter. Liveness is the mtime of `alive`, which any process
  running as the user can update.
- **Impact:** after one consent, any same-user process can (a) keep the helper alive by touching
  `alive`, (b) request a listing of any volume, and (c) read the result file. The result is the
  volume's file-name tree. No file contents are exposed, and nothing can be written through the
  helper. This is a read elevation, not a write or code-execution elevation.
- **Status:** open (design).
- **Fix options:**
  - Launch one helper per scan and exit it after the answer. This restores per-scan consent.
  - Or authenticate requests: a named pipe whose DACL allows only the owning user's SID, or a
    per-run secret passed on the command line that the helper checks.
  - Or limit the helper to the single drive it was started for.
  - Add the heartbeat check to request handling as well as liveness.

### F-02 — Medium — Helper writes into a session folder it does not verify (junction race)

- **Where:** `fast.rs:154-190` (`ensure_helper`). `create_dir_all(dir)` at line 164 accepts an existing
  directory or reparse point. The same `dir` is passed to the elevated helper in argv (173-175). The
  consent wait is up to `CONSENT_WAIT` = 180 s (line 24).
- **Evidence:** the path `%LOCALAPPDATA%\TimberStat\fast\<pid>-<nanos>` is predictable and writable
  by the user. Nothing checks that it is a real directory owned by the user before the elevated
  helper writes `ready`, `started`, `progress`, `result-N.txt` and `result-N.writing` into it.
- **Impact (plausible, needs test):** during the consent window, a same-user process could replace
  the empty session folder with a junction to another directory. The elevated helper would then
  create files there, with names chosen by the attacker (the result path comes from the request). The
  content is limited to helper-generated text. This is an elevated write to an attacker-chosen
  location.
- **Status:** open. Not reproduced. The race needs an elevated helper and cannot be tested without
  a consent prompt.
- **Fix:** create the session folder with a fresh unique name and fail if it exists. Before launch
  and again inside the helper, reject a folder that has `FILE_ATTRIBUTE_REPARSE_POINT` or whose owner
  is not the current user's SID.

### F-03 — Medium — Helper binary identity is not checked before elevation

- **Where:** `fast.rs:170` (`std::env::current_exe()`), then `elevate::launch` (`elevate.rs:47-63`,
  `runas`).
- **Evidence:** the helper is the running executable itself. Nothing verifies its signature or hash
  before `ShellExecuteExW` runs it elevated.
- **Impact:** if the folder holding the executable is writable by the user, a same-user process can
  replace the binary. The next consent prompt then runs the replacement with administrator rights. The
  Signing status of current builds was not checked in this review.
- **Status:** open.
- **Fix:** install to a protected location (for example Program Files) and Authenticode-sign release
  builds. Check the signature or a pinned hash before launch. Tie this to Plan section 20
  ("validate release artifacts").

### F-04 — Medium — Result files with full-volume listings persist after a crash

- **Where:** `fast.rs` `run_elevated` calls `paths.clear()` after reading (clear on success and on
  error). The helper folder is removed only on a clean helper exit (`fast.rs:336`).
- **Evidence:** the app and helper delete result files only on normal completion. A crash or kill
  leaves `fast\<pid>-<nanos>\result-N.txt` with the full volume tree. The helper's `remove_dir_all`
  result is ignored (`let _ =`).
- **Impact:** same-user read exposure of file names persists after a crash. Current state: the
  `fast` folder is empty on this machine, so no stale result is present.
- **Status:** open.
- **Fix:** sweep stale session folders at startup, and remove them on exit. Set an explicit ACL on
  the session folder that grants only the owning user.

### F-05 — Low — Unbounded allocations from untrusted input

- **Where:** `crates/ntfs/src/tree_io.rs:108-109` (`vec![0u8; name_len]`, `name_len` is a `u32` read
  from the result file, up to 4 GiB). `fast.rs:343` (`read_to_string` on the request file, no size
  limit).
- **Impact:** a same-user process that edits a result or request file can force a very large
  allocation in the app or the helper. A tampered file is the only input this affects.
- **Status:** open. Needs test (memory behaviour on a crafted file).
- **Fix:** cap `name_len` (Windows path limit: 32,767 UTF-16 units, so at most about 96 KiB of UTF-8).
  Cap the request file at a few KiB.

### F-06 — Medium (latent) — No archive entry-path validation; `UnsafeEntry` is never raised

- **Where:** `crates/archive/src/error.rs:47-49` declares `UnsafeEntry`. A search of the whole
  `crates` tree finds no construction of it. `engine.rs:167-199` (`extract`) runs the external tool
  with no pre-check and no post-check. The archive crate's tests are the 5 format tests in
  `format.rs`; none covers traversal, absolute paths or links.
- **Impact:** extraction safety (zip-slip, absolute paths, symlink-then-write) depends entirely on
  7-Zip, tar and unzip. No crate depends on `timberstat-archive`, so the shipped app does not call this code. The finding is latent.
- **Status:** open. Blocks Plan section 9 archive extraction.
- **Fix:** list entries first and reject absolute, drive-prefixed and `..` paths and link entries.
  After extraction, verify that every created path stays inside the destination without following
  links. Add malicious-archive fixtures for each handler family.

### F-07 — Medium (latent) — Extraction overwrites existing files, inconsistently

- **Where:** `ops.rs:96` (7-Zip extract passes `-y`), `ops.rs:136` (tar `-xf`, no `-k`), `ops.rs:193`
  (unzip extract passes `-n`). `engine.rs:167-175` creates the destination with `create_dir_all` and
  does not check that it is empty.
- **Impact:** extracting into an existing folder can silently replace files. Behaviour differs per
  handler. The 7-Zip `-y` behaviour (answer yes to overwrite prompts) is from the 7-Zip documentation
  and has not been reproduced here.
- **Status:** open.
- **Fix:** refuse a non-empty destination. Use no-overwrite flags for all handlers, or ask the user
  before replacing.

### F-08 — Low — Archive test uses a fixed, never-removed temp folder

- **Where:** `engine.rs:119-120` (`temp_dir().join("timberstat-archive-test")`). The comment says
  "drop it", but no removal follows the extraction.
- **Impact:** the name is predictable, so concurrent `test()` calls clobber each other, a same-user
  process could pre-create a junction there to redirect extraction, and extracted archive contents
  stay on disk.
- **Status:** open.
- **Fix:** use `tempfile::tempdir()` (unique, removed on drop).

### F-09 — Low — Handler output is unbounded and a killed handler may leave readers blocked

- **Where:** `runner.rs:23-29` (`read_to_end` into an unbounded `Vec`), `runner.rs:33-36`
  (`kill_child` kills only the direct child), `runner.rs:108` (`out_rx.recv()` waits for pipe EOF).
- **Impact:** a handler that writes a large amount of output can exhaust memory. If a handler spawns
  a child that inherits the pipes, `recv()` after a kill may not return. The second part is
  inferred from the code and needs test.
- **Status:** open. Needs test (hang case).
- **Fix:** cap captured bytes (for example 16 MiB per stream). Kill the process tree (job object) and
  use `recv_timeout`.

### F-10 — Low — Handlers are discovered on `PATH` and executed as the user

- **Where:** `handler.rs:162-170` (`SearchPaths::from_env` reads `PATH`). `handler_detect.rs:165-187`
  (`locate` joins each `PATH` entry with the program name). `handler_detect.rs:215-238` (`verify`
  runs the candidate with `--version`, `i`, `-v` or `-?`, 10 s timeout).
- **Impact:** any file named `7z`, `tar`, `unzip` or `UnRAR` on a `PATH` directory runs as the user
  at detection time. Relative `PATH` entries and user-writable directories make this easier. This is
  inside the same-user trust boundary but still runs code without a prompt.
- **Status:** open.
- **Fix:** prefer known install paths and skip relative `PATH` entries. Verify the publisher or a
  hash before running a discovered program.

### F-11 — Low — `file_facts` documentation and behaviour disagree for symlinks (needs test)

- **Where:** `diskprobe/src/file_facts.rs:4-5` (doc says the path is not followed). `file_facts.rs:56`
  opens the handle with `FILE_FLAG_OPEN_REPARSE_POINT`. `file_facts.rs:85`
  (`GetCompressedFileSizeW(wide.as_ptr(), ...)`) is path-based and carries no reparse flag.
- **Impact:** for a symlink or junction, `volume_serial`, `file_index` and `links` describe the link,
  while `allocated` describes the target. This is a correctness issue for storage accuracy (Plan
  section 4.4), not a security issue.
- **Status:** open. Needs test (symlink fixture requires developer mode or elevation).
- **Fix:** take the allocation size from the opened handle, or skip reparse points and report them
  as unreadable.

### F-12 — Hygiene — Missing `SAFETY` comment and misplaced doc comment in diskprobe

- **Where:** `file_facts.rs:72` (`unsafe { std::mem::zeroed() }` has no `SAFETY:` comment; all other
  blocks in the crate have one). `diskprobe/src/lib.rs:10-12` (the doc comment for `seek_penalty`
  sits above `pub mod elevate;`, so rustdoc attaches it to the wrong item).
- **Fix:** add the comment and move the doc comment.

### F-13 — Info — Helper path containment is string-based

- **Where:** `fast.rs:324` (`Path::new(&output).parent() == Some(dir.as_path())`), without
  canonicalization.
- **Assessment:** adequate for the current request format, because `..` forms fail the parent
  comparison. It only matters if F-02 is fixed, since the folder itself must be trusted first.

## Test evidence

```
cargo test -p timberstat-archive -p timberstat-diskprobe   (on git archive HEAD copy, 4m25s build)
  timberstat-archive     unittests: 5 passed, 0 failed (format::tests::*)
  timberstat-diskprobe   unittests: 1 passed, 1 ignored (tests::non_letters_are_never_asked ok;
                                    live_probe::time_real_drives ignored)
  doc-tests: 0 run
```

## Required follow-ups

1. Decide F-01 (per-scan consent or authenticated requests). It gates any release that ships the
   helper.
2. Fix F-02, F-03 and F-04 together. They all concern trust of the session folder and binary.
3. Before wiring archive features (Plan 9): F-06, F-07, F-08, F-09, F-10.
4. Add tests for F-05, F-09 (hang case) and F-11 that do not need elevation or developer mode.
5. Re-run this review after the working-tree changes above are committed, and update line numbers.
