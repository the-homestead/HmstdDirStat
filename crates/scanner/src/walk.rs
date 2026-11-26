//! Worker-side directory listing. Workers read directories and hand back plain
//! data; the coordinator in `generic.rs` owns the tree, the hard-link set and the
//! report, so nothing here needs locks.

use std::{
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
};

use timberstat_filesystem::NodeId;

use crate::types::{CancelToken, ScanOptions};

/// One directory to list, and the tree node its children will hang from.
pub struct Job {
    pub dir_id: NodeId,
    pub path: PathBuf,
}

/// One entry from a directory listing. `path` is filled only for directories,
/// because the coordinator needs it to queue them.
pub struct RawEntry {
    pub name: OsString,
    pub path: PathBuf,
    /// Taken without following links, so symlinks report their own metadata.
    pub md: fs::Metadata,
}

pub enum Item {
    Entry(RawEntry),
    /// An entry, or its metadata, could not be read.
    Failed {
        path: PathBuf,
        source: io::Error,
    },
}

/// Everything read from one directory.
pub struct Listing {
    pub dir_id: NodeId,
    pub dir_path: PathBuf,
    /// Entries dropped by an exclusion; counted so progress matches what was seen.
    pub skipped: u64,
    /// `Err` when the directory itself could not be opened.
    pub items: io::Result<Vec<Item>>,
}

/// Reads one directory. Always returns a `Listing`, even when cancelled, so the
/// coordinator never waits on a result that will not come.
pub fn list_directory(job: Job, options: &ScanOptions, cancel: &CancelToken) -> Listing {
    let Job { dir_id, path } = job;
    if cancel.is_cancelled() {
        return Listing {
            dir_id,
            dir_path: path,
            skipped: 0,
            items: Ok(Vec::new()),
        };
    }
    let listing = match fs::read_dir(&path) {
        Ok(listing) => listing,
        Err(source) => {
            return Listing {
                dir_id,
                dir_path: path,
                skipped: 0,
                items: Err(source),
            };
        }
    };

    let mut items = Vec::new();
    let mut skipped = 0;
    for item in listing {
        let entry = match item {
            Ok(entry) => entry,
            Err(source) => {
                items.push(Item::Failed {
                    path: path.clone(),
                    source,
                });
                continue;
            }
        };
        // Only build the path when an exclusion needs it; most entries never need one.
        if !options.exclude.is_empty() && is_excluded(&entry.path(), &options.exclude) {
            skipped += 1;
            continue;
        }
        match entry.metadata() {
            Ok(md) => {
                let child_path = if md.is_dir() {
                    entry.path()
                } else {
                    PathBuf::new()
                };
                items.push(Item::Entry(RawEntry {
                    name: entry.file_name(),
                    path: child_path,
                    md,
                }));
            }
            Err(source) => items.push(Item::Failed {
                path: entry.path(),
                source,
            }),
        }
    }
    Listing {
        dir_id,
        dir_path: path,
        skipped,
        items: Ok(items),
    }
}

fn is_excluded(path: &Path, exclude: &[PathBuf]) -> bool {
    exclude.iter().any(|prefix| path.starts_with(prefix))
}
