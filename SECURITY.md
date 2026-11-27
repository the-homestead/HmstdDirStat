# Security Policy

TimberStat reads NTFS metadata through an elevated helper, runs external archive tools, and
uses a small amount of `unsafe` Windows FFI. This file states what is trusted, what is not, and
how to report a problem. The full review record is in [docs/SECURITY_REVIEW.md](docs/SECURITY_REVIEW.md).

## Supported versions

TimberStat is pre-1.0. Only the current `main` branch is supported. Security fixes land on `main`;
there are no backports.

## Reporting a vulnerability

Do not open a public issue for a security problem.

Report privately through GitHub private vulnerability reporting on
`the-homestead/TimberStat` (Security tab, "Report a vulnerability").

Include:

- TimberStat commit hash and Windows version.
- Steps to reproduce, and the observed and expected result.
- Impact as you understand it (for example: which user or process is affected).

## Trust model

### Same-user processes are inside the trust boundary

TimberStat does not defend against code already running as the logged-in user. Anything that can
write to the user's profile can also read TimberStat's files and send it requests. The points below
describe what that means for each feature.

### Elevated Fast-scan helper

- **What it does.** On a Fast scan, TimberStat starts one elevated copy of itself through the
  Windows consent prompt (`runas`, window hidden, `--fast-service <dir>`). The helper reads the
  NTFS master file table for one drive and writes a summary and folder tree. It is started once per
  app run.
- **What it reads.** File-table metadata only: names, sizes, timestamps and link counts, for any
  drive letter. It does not read file contents. The `timberstat-ntfs` crate has no `unsafe` code and
  reads boot-sector and MFT records only.
- **What it accepts.** Only drive volumes (`//./X:`). The result path must be a direct child of the
  helper's folder. The helper writes only inside that folder.
- **Lifetime.** The helper exits 30 seconds after the app stops refreshing its `alive` file.
- **Same-user read exposure.** One consent grants every process running as the same user a way to
  ask the helper for a listing of any drive, for as long as the helper stays up. Requests,
  heartbeat and results are plain files in `%LOCALAPPDATA%\TimberStat\fast\<pid>-<nanos>\`. The
  helper does not authenticate requests. A result file contains the names of files on the whole
  volume, including other users' folders. The app deletes result files after a normal read, but a
  crash can leave them behind.
- **What this does not allow.** Same-user code cannot read file contents through the helper and
  cannot run code in the helper. It also cannot change files on the volume through it.

Open items for this surface are tracked as F-01 to F-04 in the review record.

### Unsafe code

- Only `timberstat-diskprobe` may use `unsafe`. Its `Cargo.toml` sets `unsafe_code = "allow"`. Every
  other crate that inherits the workspace lints has `unsafe_code = "forbid"`.
- The crate has 15 `unsafe` blocks across three files:
  - `elevate.rs` (5): `ShellExecuteExW`, `WaitForSingleObject`, `CloseHandle`, `GetLastError`.
  - `lib.rs` (4): `GetDiskFreeSpaceExW`, `CreateFileW` with zero access, the seek-penalty
    `DeviceIoControl`, `CloseHandle`.
  - `file_facts.rs` (6): `CreateFileW` with `FILE_READ_ATTRIBUTES` only, `GetFileInformationByHandle`,
    `GetCompressedFileSizeW`, `GetLastError`, `CloseHandle`, and one `zeroed()`.
- Every volume or file opened by this crate is opened without write access. The only device control
  used is the read-only `IOCTL_STORAGE_QUERY_PROPERTY`. No write, FSCTL or set-information call exists
  in the crate.
- Changes to this crate need a reviewer who reads each `unsafe` block against its `SAFETY:` comment.

### Archive handlers

- **Current status.** `timberstat-archive` is a library crate. No other crate in this workspace
  depends on it, so the shipped app does not run archive handlers yet. The points below apply before
  archive features (Plan section 9) are wired in.
- **How it runs tools.** Archive work calls external programs (7-Zip family, tar/bsdtar, unzip,
  UnRAR) with an argument vector. No shell string is built.
- **Path safety is delegated.** TimberStat does not validate archive entry paths before or after
  extraction. The `UnsafeEntry` error variant exists but is never raised, and no traversal or
  symlink tests exist. Safety depends on the external tool.
- **Overwrite.** Extraction does not check that the destination is empty. 7-Zip is run with `-y`
  and tar overwrites by default; unzip is run with `-n`.
- **Handler discovery.** Detection runs candidate programs found on `PATH` with a version argument
  and treats a successful version probe as a valid handler.
- **Required before wiring.** Entry validation (reject absolute, drive-prefixed and `..` paths and
  link entries), no-overwrite by default, an empty destination, and malicious-archive fixtures in
  tests. Tracked as F-06 to F-10.

## Known limitations

- The helper's binary identity is not checked before elevation (F-03). Signing status of current
  builds has not been confirmed.
- The helper runs only on Windows. Other platforms return no data for these calls.
- Installing the app outside a user-writable folder is the main mitigation for F-03. It is not yet
  enforced.
