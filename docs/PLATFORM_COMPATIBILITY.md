# PLATFORM COMPATIBILITY — TimberStat

Primary: Windows 10/11. Secondary: Linux. Unavailable or reduced functionality must be shown to the user, not hidden (Plan §17).

Status legend: **V** verified against a primary source; **P** planned; **U** unknown, needs testing.

## 1. Build and toolchain

| Item | Status | Note |
|---|---|---|
| Windows MSVC toolchain (`stable-x86_64-pc-windows-msvc`) | V (local) | rustc 1.100.0 installed |
| Linux GNU toolchain | P | Upstream GPUI Kit CI builds Linux; our CI job to confirm |
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
| Mount discovery | 2 | `/proc/self/mountinfo` (planned) |
| Mount boundary stop | 2 | `st_dev` change |
| Trash | 5 | freedesktop trash spec (planned) |
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
