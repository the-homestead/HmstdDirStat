# PLATFORM COMPATIBILITY — TimberStat

Primary: Windows 10/11. Secondary: Linux. Unavailable or reduced functionality must be shown to the user, not hidden (Plan §17).

Status legend: **V** verified against a primary source; **P** planned; **U** unknown, needs testing.

## 1. Build and toolchain

| Item | Status | Note |
|---|---|---|
| Windows MSVC toolchain (`stable-x86_64-pc-windows-msvc`) | V (local) | rustc 1.100.0 installed |
| Linux GNU toolchain | P | Upstream GPUI Kit CI builds Linux; our CI job to confirm. Cross type-check from Windows: V (P5.10, see §7) |
| MSRV 1.95 | U | See DEPENDENCY_SELECTION §4 |
| Edition 2024 | V | Required by gpui-kit 0.11.0 |
| Git | V | git 2.53.0 windows |

## 2. Windows capability matrix

| Capability | Phase | Requirement | Status |
|---|---|---|---|
| Generic traversal (`std::fs`) | 2 | none | P |
| Volume discovery | 2 | Win32 volume APIs | P |
| NTFS MFT / USN enumeration (`FSCTL_ENUM_USN_DATA`) | 3 | NTFS volume; usually elevation | P; fallback mandatory |
| Per-file sizes by ID (`OpenFileById`) | 3 | file access rights | P |
| Hard links | 2–3 | file id + volume serial | P |
| Symlinks / junctions / reparse points | 2 | no follow by default; `FSCTL_GET_REPARSE_POINT` | P |
| Sparse / compressed | 2 | allocated size via `GetCompressedFileSizeW` where needed | P |
| Cloud placeholders (OneDrive etc.) | 2 | detect via attributes; never hydrate during scan | P |
| Long paths `\\?\` | 2 | required beyond MAX_PATH | P |
| Recycle Bin | 5 | Shell API | P |
| UAC elevation | 3 | only for raw volume path; app runs unelevated by default | P |
| Explorer context menu / file association | 8 | registry/shell integration | P |
| Native file dialogs | 5 | Win32 common dialogs | P |

### Filesystems

| FS | Enumeration | Notes |
|---|---|---|
| NTFS | fast path + generic | primary target |
| ReFS | generic only | `FSCTL_ENUM_USN_DATA` not supported; report fallback |
| FAT32 / exFAT | generic | no hard links, coarse timestamps |
| SMB / network | generic only | no USN; slow; report as reduced |
| CSVFS | generic (USN possible) | not tested; U |

## 3. GPU and rendering (GPUI)

- GPUI renders via the GPU backend provided by `gpui-pre-platform`. Windows uses D3D-class drivers; the
  exact backend and minimum driver level are **U** and must be documented from the verified crate
  before a release claim.
- Software-only or RDP sessions may render slowly or fail; the app must start and report the renderer failure
  rather than crash silently. **U** — test on a VM.
- Linux requires a windowing system (X11 or Wayland) and a Vulkan- or GL-capable driver. **U**.

## 4. Linux capability matrix

| Capability | Phase | Requirement |
|---|---|---|
| Enumeration | 2 | `read_dir` + `symlink_metadata` |
| Allocated size | 2 | `st_blocks * 512` approximation, documented |
| Permissions / owner | 5 | `st_mode`, `st_uid` |
| Symlinks | 2 | not followed by default |
| Mount discovery | 2 | `/proc/self/mountinfo`, kernel pseudo filesystems skipped (implemented, P5.10) |
| Mount boundary stop | 2 | `st_dev` change |
| Trash | 5 | freedesktop trash spec via the `trash` crate (implemented, P5.10; runtime not verified on Linux) |
| Special files (FIFO, socket, device) | 2 | record, never open |
| Deleted-but-open files | 2 | not visible; documented limitation |

## 5. Explicitly unsupported in early phases

- Raw MFT scanning on Linux (no equivalent; not applicable).
- Recycle Bin on Linux (uses freedesktop trash instead).
- Hard-link physical accounting on FAT/exFAT (no link ids).

## 6. Test coverage by platform

Windows and Linux are both exercised in CI (Phase 1 gate). Elevated NTFS, reparse and
network-share tests run in separate, opt-in lanes; they are **not run** in default CI and
must be reported as not run when they are skipped.

## 7. Linux status and reduced features (P5.10)

### Verification status

| Check | Status | Evidence |
|---|---|---|
| Windows `cargo fmt --all -- --check` | green | run on this host |
| Windows `cargo clippy --workspace --all-targets -- -D warnings` | green | run on this host |
| Windows `cargo test --workspace` | green | 58 suites, 348 passed, 0 failed, 1 ignored |
| Linux type-check `cargo check --workspace --all-targets --locked --target x86_64-unknown-linux-gnu` | green (Rust only) | run from the Windows host, see note |
| Linux clippy `cargo clippy --workspace --all-targets --target x86_64-unknown-linux-gnu -- -D warnings` | green (Rust only) | run from the Windows host, see note |
| Linux `cargo test --workspace` | **Not verified** | no Linux runtime here (WSL distros do not start) |
| Ubuntu CI job | **Not verified** | needs a push, which the owner rule forbids for this task |
| Linux GUI | **Not verified** | owner rule: no desktop control |

Note on the cross checks: the Linux target was checked from the Windows host with the Rust
toolchain only. C build scripts (freetype, fontconfig, wayland-backend) ran against a check-only
stand-in compiler that emits empty objects, so nothing was linked and no test binary was run.
This proves Rust type-checking and `cfg` coverage for Linux. It does not prove linking or behaviour.

### `cfg` gates added or changed for Linux

- `crates/app/src/platform.rs` (new): `TRASH_NAME` (`Recycle Bin` / `Trash`) and `REVEAL_LABEL`
  (`Show in Explorer` / `Open containing folder`) are `cfg(windows)` / `cfg(not(windows))`;
  `FAST_NTFS_AVAILABLE` is `cfg!(windows)`.
- `crates/fileops/src/model.rs`: the Recycle Bin failure text uses `cfg!(windows)` (`Trash rejected
  the item` on Linux).
- `crates/scanner/src/volumes.rs`: pseudo filesystem filter and `mounts_from_mountinfo` are
  `cfg(any(test, target_os = "linux"))`. Their unit tests are pure string tests and run on Windows too.
- Test helper `running_as_root()` (reads `/proc/self/status`, always false off Linux): root ignores
  permission bits, so the chmod-based tests in `crates/duplicates/tests/pipeline.rs` and
  `crates/fileops/tests/delete.rs` print a skip message and return when run as root.

Existing gates already in place and checked on Linux: `scanner/src/volumes.rs`,
`scanner/src/generic.rs` (`st_dev`, `st_ino`, `st_blocks`, unix mode), `fileops/src/disk.rs`,
`archive/src/handler.rs` (PATH lookup), `storage-insights/src/locations.rs` and
`dev-storage/src/locations.rs` (XDG and `/tmp`, `/var/log`), `app/src/open.rs` (`xdg-open`).

### Linux feature behaviour

| Feature | Linux behaviour | Status |
|---|---|---|
| Fast NTFS (MFT read) | **Unavailable.** The toolbar control is disabled with the reason as its tooltip. `set_fast_mode(true)` is refused. | unavailable, shown with reason |
| Show in Explorer | Reduced: `Open containing folder` opens the parent through `xdg-open`. It does not select the item. | reduced, labelled |
| Delete to bin | freedesktop trash through the `trash` crate. UI says `Trash`. Permanent delete unchanged. | implemented, runtime not verified |
| Volumes | Mount points from `/proc/self/mountinfo`, deduplicated. Pseudo filesystems (proc, sysfs, cgroup, devpts, ...) are excluded. No size or removable metadata. | implemented, runtime not verified |
| Physical size | `st_blocks * 512` | implemented |
| Hard links | `(st_dev, st_ino)` identity, so no double count | implemented |
| Permissions | `st_mode` recorded as `unix_mode` | implemented |
| Special files | Recorded as `Other`, never opened | implemented |
| Archives | `7z`, `7za`, `7zz`, `7zr`, `tar`, `unzip` found on `PATH`. The Windows install paths and the GUI-only NanaZip and WinRAR handlers are not searched. | implemented, runtime not verified |
| Caches, logs, crash dumps, downloads, game libraries | XDG cache via `directories`, `/tmp`, `/var/tmp`, `/var/log`, systemd coredump, `/var/crash`, `~/Downloads`, Steam library paths | implemented, runtime not verified |
| Elevation / UAC, registry, shell integration | not applicable on Linux | n/a |

### Tests that are Not verified on a Windows host

- `crates/duplicates/tests/pipeline.rs::unreadable_file_is_reported_and_left_out_of_groups` is
  `cfg(unix)` and uses `chmod 000`. It does not run on Windows at all, and no Windows equivalent
  exists. It is Not verified on a Windows host. Linux runtime is Not verified too.
- `crates/fileops/tests/delete.rs::partial_failure_...` runs on Windows with a sharing lock. Its
  `chmod 0o555` branch is `cfg(unix)` and does not run on Windows.

