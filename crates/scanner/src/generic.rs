use std::{
    collections::HashSet,
    ffi::OsStr,
    fs,
    path::Path,
    sync::{Mutex, mpsc},
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

use timberstat_filesystem::{EntryKind, FileIdentity, FileTree, Metadata, NewEntry, NodeId};

use crate::{
    FileSystemScanner,
    types::{
        CancelToken, ScanError, ScanMethod, ScanOptions, ScanOutput, ScanProgress, ScanReport,
    },
    walk::{Item, Job, Listing, RawEntry, list_directory},
};

/// Progress is reported every this many entries.
const PROGRESS_INTERVAL: u64 = 4096;

/// Portable `std::fs` traversal. Metadata only; never opens file contents.
///
/// Directories are listed on worker threads (see [`ScanOptions::threads`]). A
/// single coordinator thread builds the tree in arrival order, so tree ids, the
/// hard-link set and the report are only ever touched from one place.
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
    let tree = FileTree::with_root(NewEntry {
        name: root_name,
        kind: EntryKind::Directory,
        logical_size: 0,
        physical_size: physical_of(&root_meta),
        identity: None,
        hardlink_duplicate: false,
        unreadable: false,
        metadata: metadata_of(&root_meta),
    });
    let mut coordinator = Coordinator {
        tree,
        report: ScanReport::new(ScanMethod::GenericTraversal),
        seen_identities: HashSet::new(),
        directories_read: 0,
        root_device: device_of(&root_meta),
        options,
    };

    let workers = options.worker_count();
    let (job_tx, job_rx) = mpsc::channel::<Job>();
    let job_rx = Mutex::new(job_rx);
    let (result_tx, result_rx) = mpsc::channel::<Listing>();
    let jobs = &job_rx;

    // The closure takes ownership of the job sender. Dropping it when the closure
    // returns is what lets the workers exit, so the scope can join them.
    let walked = thread::scope(move |scope| -> Result<Coordinator<'_>, ScanError> {
        for _ in 0..workers {
            let results = result_tx.clone();
            scope.spawn(move || worker(jobs, results, options, cancel));
        }
        drop(result_tx);

        let _ = job_tx.send(Job {
            dir_id: NodeId::ROOT,
            path: root.to_path_buf(),
        });
        let mut outstanding: usize = 1;
        let mut queued: Vec<Job> = Vec::new();
        while outstanding > 0 {
            check_cancel(cancel)?;
            let Ok(listing) = result_rx.recv() else {
                return Err(ScanError::WorkersStopped);
            };
            outstanding -= 1;
            coordinator.apply(listing, cancel, progress, &mut queued)?;
            for job in queued.drain(..) {
                outstanding += 1;
                let _ = job_tx.send(job);
            }
        }
        progress(ScanProgress {
            entries_visited: coordinator.report.entries_visited,
            directories_read: coordinator.directories_read,
        });
        // The final progress callback may itself request cancellation.
        check_cancel(cancel)?;
        Ok(coordinator)
    });

    let mut coordinator = walked?;
    coordinator.tree.roll_up();
    Ok(ScanOutput {
        tree: coordinator.tree,
        report: coordinator.report,
    })
}

/// Pulls directories off the shared queue and sends back their listings until
/// the queue closes.
fn worker(
    jobs: &Mutex<mpsc::Receiver<Job>>,
    results: mpsc::Sender<Listing>,
    options: &ScanOptions,
    cancel: &CancelToken,
) {
    loop {
        let job = match jobs.lock() {
            Ok(receiver) => receiver.recv(),
            Err(_) => break,
        };
        let Ok(job) = job else { break };
        if results.send(list_directory(job, options, cancel)).is_err() {
            break;
        }
    }
}

/// Owns everything that must be updated in one place: the tree, the hard-link
/// set, and the report. Runs on the scanning thread only.
struct Coordinator<'a> {
    tree: FileTree,
    report: ScanReport,
    seen_identities: HashSet<FileIdentity>,
    directories_read: u64,
    root_device: Option<u64>,
    options: &'a ScanOptions,
}

impl Coordinator<'_> {
    /// Adds one listing to the tree and queues any subdirectories it found.
    fn apply(
        &mut self,
        listing: Listing,
        cancel: &CancelToken,
        progress: &mut dyn FnMut(ScanProgress),
        queued: &mut Vec<Job>,
    ) -> Result<(), ScanError> {
        let Listing {
            dir_id,
            dir_path,
            skipped,
            items,
        } = listing;
        self.report.entries_visited += skipped;

        let items = match items {
            Ok(items) => {
                self.directories_read += 1;
                items
            }
            Err(source) => {
                self.tree.mark_unreadable(dir_id)?;
                self.report.push_issue(&dir_path, source);
                return Ok(());
            }
        };

        for item in items {
            check_cancel(cancel)?;
            self.report.entries_visited += 1;
            if self
                .report
                .entries_visited
                .is_multiple_of(PROGRESS_INTERVAL)
            {
                progress(ScanProgress {
                    entries_visited: self.report.entries_visited,
                    directories_read: self.directories_read,
                });
            }
            match item {
                Item::Entry(raw) => self.add_entry(dir_id, raw, queued)?,
                Item::Failed { path, source } => {
                    // An entry we cannot see means this directory's totals are incomplete.
                    self.tree.mark_unreadable(dir_id)?;
                    self.report.push_issue(&path, source);
                }
            }
        }
        Ok(())
    }

    fn add_entry(
        &mut self,
        dir_id: NodeId,
        raw: RawEntry,
        queued: &mut Vec<Job>,
    ) -> Result<(), ScanError> {
        let md = &raw.md;
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
            EntryKind::File => md.len(),
            // A symlink's own length is its target path, not file data, so it adds nothing.
            EntryKind::Symlink => 0,
            EntryKind::Directory | EntryKind::Other => 0,
        };

        let extras = platform_extras(
            md,
            kind,
            self.root_device,
            self.options,
            &mut self.seen_identities,
            &mut self.report,
        );
        let physical = if extras.hardlink_duplicate {
            Some(0)
        } else {
            physical_of(md)
        };

        let child = self.tree.add_child(
            dir_id,
            NewEntry {
                name: raw.name,
                kind,
                logical_size,
                physical_size: physical,
                identity: extras.identity,
                hardlink_duplicate: extras.hardlink_duplicate,
                unreadable: false,
                metadata: metadata_of(md),
            },
        )?;

        if extras.descend {
            queued.push(Job {
                dir_id: child,
                path: raw.path,
            });
        }
        Ok(())
    }
}

fn check_cancel(cancel: &CancelToken) -> Result<(), ScanError> {
    if cancel.is_cancelled() {
        Err(ScanError::Cancelled)
    } else {
        Ok(())
    }
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
