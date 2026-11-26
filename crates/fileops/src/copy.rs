//! Disk-level copy and removal used by copy and by cross-volume move.
//!
//! Copy never overwrites by accident. A file is created with `create_new`, so a
//! file that appears between the check and the write is refused, not replaced.
//! Overwrite writes a staged file first and replaces the target only after the
//! copy finished.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use crate::disk::{is_dir_entry, is_link_like, staging_path};
use crate::model::{Control, Failure, FailureReason};
use crate::transfer::{SkipReason, Skipped};

/// What a copy did, merged into the transfer report by the caller.
#[derive(Debug, Default)]
pub(crate) struct CopyTally {
    pub files: u64,
    pub bytes: u64,
    pub skipped: Vec<Skipped>,
    pub failures: Vec<Failure>,
}

/// Copies `src` to `dst`. Returns true only when every entry under `src` was copied.
///
/// `overwrite` allows replacing existing files and merging into existing folders.
/// Links and reparse points are skipped and reported, never followed.
pub(crate) fn copy_entry(
    src: &Path,
    dst: &Path,
    overwrite: bool,
    control: &Control,
    tally: &mut CopyTally,
) -> bool {
    if control.is_cancelled() {
        return false;
    }
    let meta = match fs::symlink_metadata(src) {
        Ok(meta) => meta,
        Err(error) => {
            tally.failures.push(Failure::from_io(src, &error));
            return false;
        }
    };
    if is_link_like(&meta) {
        tally.skipped.push(Skipped {
            path: src.to_path_buf(),
            reason: SkipReason::LinkNotCopied,
        });
        return false;
    }
    if is_dir_entry(&meta) {
        return copy_folder(src, dst, overwrite, control, tally);
    }
    if !meta.file_type().is_file() {
        tally.skipped.push(Skipped {
            path: src.to_path_buf(),
            reason: SkipReason::NotRegularFile,
        });
        return false;
    }
    copy_file(src, dst, overwrite, control, tally)
}

fn copy_folder(
    src: &Path,
    dst: &Path,
    overwrite: bool,
    control: &Control,
    tally: &mut CopyTally,
) -> bool {
    match fs::symlink_metadata(dst) {
        Ok(existing) if is_dir_entry(&existing) => {
            if !overwrite {
                tally.skipped.push(Skipped {
                    path: dst.to_path_buf(),
                    reason: SkipReason::CollisionSkipped,
                });
                return false;
            }
        }
        Ok(_) => {
            tally.failures.push(Failure::new(
                dst,
                FailureReason::Conflict,
                "destination exists and is not a folder",
            ));
            return false;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            if let Err(error) = fs::create_dir(dst) {
                tally.failures.push(Failure::from_io(dst, &error));
                return false;
            }
        }
        Err(error) => {
            tally.failures.push(Failure::from_io(dst, &error));
            return false;
        }
    }
    let entries = match fs::read_dir(src) {
        Ok(entries) => entries,
        Err(error) => {
            tally.failures.push(Failure::from_io(src, &error));
            return false;
        }
    };
    let mut complete = true;
    for entry in entries {
        match entry {
            Ok(entry) => {
                let child_dst = dst.join(entry.file_name());
                complete &= copy_entry(&entry.path(), &child_dst, overwrite, control, tally);
            }
            Err(error) => {
                tally.failures.push(Failure::from_io(src, &error));
                complete = false;
            }
        }
    }
    complete
}

fn copy_file(
    src: &Path,
    dst: &Path,
    overwrite: bool,
    control: &Control,
    tally: &mut CopyTally,
) -> bool {
    let existing = match fs::symlink_metadata(dst) {
        Ok(existing) => Some(existing),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => {
            tally.failures.push(Failure::from_io(dst, &error));
            return false;
        }
    };
    let replacing = match existing {
        None => false,
        Some(ref meta) if is_dir_entry(meta) || is_link_like(meta) => {
            tally.failures.push(Failure::new(
                dst,
                FailureReason::Conflict,
                "destination is a folder or link; not replaced",
            ));
            return false;
        }
        Some(_) if !overwrite => {
            tally.skipped.push(Skipped {
                path: dst.to_path_buf(),
                reason: SkipReason::CollisionSkipped,
            });
            return false;
        }
        Some(_) => true,
    };

    let result = if replacing {
        copy_then_replace(src, dst)
    } else {
        copy_file_new(src, dst)
    };
    match result {
        Ok(bytes) => {
            tally.files += 1;
            tally.bytes = tally.bytes.saturating_add(bytes);
            control.advance(1);
            true
        }
        Err(error) => {
            tally.failures.push(Failure::from_io(src, &error));
            false
        }
    }
}

/// Copies into a new file. Fails if the destination already exists.
fn copy_file_new(src: &Path, dst: &Path) -> io::Result<u64> {
    let mut input = File::open(src)?;
    let permissions = input.metadata()?.permissions();
    let mut output = OpenOptions::new().write(true).create_new(true).open(dst)?;
    let copied = io::copy(&mut input, &mut output);
    let copied = match copied {
        Ok(bytes) => bytes,
        Err(error) => {
            drop(output);
            let _ = fs::remove_file(dst);
            return Err(error);
        }
    };
    drop(output);
    fs::set_permissions(dst, permissions)?;
    Ok(copied)
}

/// Stages the copy beside the target, then renames it over the target.
fn copy_then_replace(src: &Path, dst: &Path) -> io::Result<u64> {
    let name = dst.file_name().unwrap_or(dst.as_os_str());
    let staged = staging_path(dst, name);
    let copied = match copy_file_new(src, &staged) {
        Ok(bytes) => bytes,
        Err(error) => {
            let _ = fs::remove_file(&staged);
            return Err(error);
        }
    };
    if let Err(error) = fs::rename(&staged, dst) {
        let _ = fs::remove_file(&staged);
        return Err(error);
    }
    Ok(copied)
}

/// Removes everything under `path` from disk, children first. Returns one
/// failure per entry that could not be removed. An empty return means the
/// whole tree is gone.
pub(crate) fn remove_tree_on_disk(path: &Path) -> Vec<Failure> {
    let mut failures = Vec::new();
    // Pre-order walk without recursion, so depth is not limited by the stack.
    let mut order: Vec<(PathBuf, bool)> = Vec::new();
    let mut pending = vec![path.to_path_buf()];
    while let Some(current) = pending.pop() {
        let meta = match fs::symlink_metadata(&current) {
            Ok(meta) => meta,
            Err(error) => {
                failures.push(Failure::from_io(&current, &error));
                continue;
            }
        };
        if is_dir_entry(&meta) {
            order.push((current.clone(), true));
            match fs::read_dir(&current) {
                Ok(entries) => {
                    for entry in entries {
                        match entry {
                            Ok(entry) => pending.push(entry.path()),
                            Err(error) => failures.push(Failure::from_io(&current, &error)),
                        }
                    }
                }
                Err(error) => failures.push(Failure::from_io(&current, &error)),
            }
        } else {
            order.push((current, false));
        }
    }
    // Reversed pre-order puts every child before its parent.
    for (entry, is_dir) in order.into_iter().rev() {
        let result = if is_dir {
            fs::remove_dir(&entry)
        } else {
            let meta = fs::symlink_metadata(&entry);
            match meta {
                Ok(meta) if crate::disk::is_link_to_dir(&meta) => fs::remove_dir(&entry),
                _ => fs::remove_file(&entry),
            }
        };
        if let Err(error) = result
            && error.kind() != io::ErrorKind::NotFound
        {
            failures.push(Failure::from_io(&entry, &error));
        }
    }
    failures
}
