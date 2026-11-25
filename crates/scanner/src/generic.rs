use std::{
    collections::HashSet,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use hmstd_filesystem::{EntryKind, FileIdentity, FileTree, Metadata, NewEntry, NodeId};

use crate::{
    FileSystemScanner,
    types::{
        CancelToken, ScanError, ScanMethod, ScanOptions, ScanOutput, ScanProgress, ScanReport,
    },
};

/// Progress is reported every this many entries.
const PROGRESS_INTERVAL: u64 = 4096;

/// Portable `std::fs` traversal. Metadata only; never opens file contents.
#[derive(Debug, Clone, Copy, Default)]
pub struct GenericScanner;

impl FileSystemScanner for GenericScanner {
    fn method(&self) -> ScanMethod {
        ScanMethod::GenericTraversal
    }

    fn scan(
        &self,
        root: &Path,
        options: &ScanOptions,
        cancel: &CancelToken,
        progress: &mut dyn FnMut(ScanProgress),
    ) -> Result<ScanOutput, ScanError> {
        scan_generic(root, options, cancel, progress)
    }
}

fn scan_generic(
    root: &Path,
    options: &ScanOptions,
    cancel: &CancelToken,
    progress: &mut dyn FnMut(ScanProgress),
) -> Result<ScanOutput, ScanError> {
    // The root is followed, so a link to a directory can be scanned as the root itself.
    let root_meta = fs::metadata(root).map_err(|source| ScanError::RootUnavailable {
        path: root.to_path_buf(),
        source,
    })?;
    if !root_meta.is_dir() {
        return Err(ScanError::RootNotDirectory(root.to_path_buf()));
    }
    check_cancel(cancel)?;

    let root_name = root
        .file_name()
        .map_or_else(|| root.as_os_str().to_os_string(), OsStr::to_os_string);
    let root_device = device_of(&root_meta);
    let mut tree = FileTree::with_root(NewEntry {
        name: root_name,
        kind: EntryKind::Directory,
        logical_size: 0,
        physical_size: physical_of(&root_meta),
        identity: None,
        hardlink_duplicate: false,
        unreadable: false,
        metadata: metadata_of(&root_meta),
    });

    let mut report = ScanReport::new(ScanMethod::GenericTraversal);
    let mut seen_identities: HashSet<FileIdentity> = HashSet::new();
    let mut directories_read: u64 = 0;
    // Explicit stack instead of recursion: depth is limited by memory, not the thread stack.
    let mut stack: Vec<(NodeId, PathBuf)> = vec![(NodeId::ROOT, root.to_path_buf())];

    while let Some((dir_id, dir_path)) = stack.pop() {
        check_cancel(cancel)?;

        let listing = match fs::read_dir(&dir_path) {
            Ok(listing) => listing,
            Err(source) => {
                tree.mark_unreadable(dir_id)?;
                report.push_issue(&dir_path, source);
                continue;
            }
        };
        directories_read += 1;

        for item in listing {
            check_cancel(cancel)?;
            report.entries_visited += 1;
            if report.entries_visited.is_multiple_of(PROGRESS_INTERVAL) {
                progress(ScanProgress {
                    entries_visited: report.entries_visited,
                    directories_read,
                });
            }

            let entry = match item {
                Ok(entry) => entry,
                Err(source) => {
                    // An entry we cannot see means this directory's totals are incomplete.
                    tree.mark_unreadable(dir_id)?;
                    report.push_issue(&dir_path, source);
                    continue;
                }
            };
            let path = entry.path();
            if is_excluded(&path, &options.exclude) {
                continue;
            }

            let md = match fs::symlink_metadata(&path) {
                Ok(md) => md,
                Err(source) => {
                    tree.mark_unreadable(dir_id)?;
                    report.push_issue(&path, source);
                    continue;
                }
            };

            let file_type = md.file_type();
            let kind = if file_type.is_symlink() {
                EntryKind::Symlink
            } else if file_type.is_dir() {
                EntryKind::Directory
            } else if file_type.is_file() {
                EntryKind::File
            } else {
                EntryKind::Other
            };
            let logical_size = match kind {
                EntryKind::File | EntryKind::Symlink => md.len(),
                EntryKind::Directory | EntryKind::Other => 0,
            };

            let extras = platform_extras(
                &md,
                kind,
                root_device,
                options,
                &mut seen_identities,
                &mut report,
            );
            let physical = if extras.hardlink_duplicate {
                Some(0)
            } else {
                physical_of(&md)
            };

            let child = tree.add_child(
                dir_id,
                NewEntry {
                    name: entry.file_name(),
                    kind,
                    logical_size,
                    physical_size: physical,
                    identity: extras.identity,
                    hardlink_duplicate: extras.hardlink_duplicate,
                    unreadable: false,
                    metadata: metadata_of(&md),
                },
            )?;

            if extras.descend {
                stack.push((child, path));
            }
        }
    }

    progress(ScanProgress {
        entries_visited: report.entries_visited,
        directories_read,
    });
    tree.roll_up();
    Ok(ScanOutput { tree, report })
}

fn check_cancel(cancel: &CancelToken) -> Result<(), ScanError> {
    if cancel.is_cancelled() {
        Err(ScanError::Cancelled)
    } else {
        Ok(())
    }
}

fn is_excluded(path: &Path, exclude: &[PathBuf]) -> bool {
    exclude.iter().any(|prefix| path.starts_with(prefix))
}

/// Per-entry decisions that depend on the platform.
struct Extras {
    identity: Option<FileIdentity>,
    /// This path is a later hard link to a file already charged; its physical size is zero.
    hardlink_duplicate: bool,
    /// Whether to enter this directory.
    descend: bool,
}

#[cfg(unix)]
fn platform_extras(
    md: &fs::Metadata,
    kind: EntryKind,
    root_device: Option<u64>,
    options: &ScanOptions,
    seen: &mut HashSet<FileIdentity>,
    report: &mut ScanReport,
) -> Extras {
    use std::os::unix::fs::MetadataExt;

    let mut extras = Extras {
        identity: None,
        hardlink_duplicate: false,
        descend: kind == EntryKind::Directory,
    };
    if kind == EntryKind::Directory && !options.cross_mount_points && root_device != Some(md.dev())
    {
        extras.descend = false;
        report.mount_points_skipped += 1;
    }
    // Only multiply-linked files need an identity; single links cannot be double-counted.
    if kind == EntryKind::File && md.nlink() > 1 {
        let identity = FileIdentity {
            volume: md.dev(),
            file_id: md.ino(),
        };
        extras.identity = Some(identity);
        if !seen.insert(identity) {
            extras.hardlink_duplicate = true;
            report.hardlink_duplicates += 1;
        }
    }
    extras
}

#[cfg(not(unix))]
fn platform_extras(
    _md: &fs::Metadata,
    kind: EntryKind,
    _root_device: Option<u64>,
    _options: &ScanOptions,
    _seen: &mut HashSet<FileIdentity>,
    _report: &mut ScanReport,
) -> Extras {
    // Windows file identity (volume serial + file id) is added with the NTFS
    // scanner in Phase 3. Until then hard links are counted per path and physical
    // sizes are reported as unknown.
    Extras {
        identity: None,
        hardlink_duplicate: false,
        descend: kind == EntryKind::Directory,
    }
}

#[cfg(unix)]
fn device_of(md: &fs::Metadata) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    Some(md.dev())
}

#[cfg(not(unix))]
fn device_of(_md: &fs::Metadata) -> Option<u64> {
    None
}

/// Physical bytes allocated on disk, from `st_blocks` (512-byte units).
#[cfg(unix)]
fn physical_of(md: &fs::Metadata) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    Some(md.blocks().saturating_mul(512))
}

/// Not available from `std` on Windows; reported as unknown until the NTFS path lands.
#[cfg(not(unix))]
fn physical_of(_md: &fs::Metadata) -> Option<u64> {
    None
}

fn metadata_of(md: &fs::Metadata) -> Metadata {
    let mut metadata = Metadata {
        modified_unix_secs: md.modified().ok().and_then(unix_seconds),
        readonly: md.permissions().readonly(),
        unix_mode: None,
        windows_attributes: None,
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.unix_mode = Some(md.permissions().mode());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.windows_attributes = Some(md.file_attributes());
    }
    metadata
}

fn unix_seconds(time: SystemTime) -> Option<i64> {
    match time.duration_since(UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_secs()).ok(),
        Err(before) => i64::try_from(before.duration().as_secs())
            .ok()
            .and_then(i64::checked_neg),
    }
}
