//! Rename of one entry in place, with the same collision policies as copy.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use crate::disk::{free_name, is_dir_entry, is_link_like, same_location};
use crate::model::{Failure, FailureReason, PlanError};
use crate::transfer_plan::Collision;

/// Result of a rename that did not fail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenameOutcome {
    /// The entry now has this path.
    Renamed { to: PathBuf },
    /// The new name equals the old one. Nothing changed.
    Unchanged,
    /// The name was taken and the policy was skip. Nothing changed.
    Skipped { existing: PathBuf },
}

/// Checks a file name for Windows and POSIX validity. Refuses separators,
/// reserved characters, control characters, and names ending in a dot or space.
pub fn validate_name(name: &str) -> Result<(), PlanError> {
    const RESERVED: &[char] = &['<', '>', ':', '"', '|', '?', '*', '/', '\\'];
    if name.trim().is_empty() {
        return Err(PlanError::BadName(name.to_owned(), "the name is empty"));
    }
    if name == "." || name == ".." {
        return Err(PlanError::BadName(name.to_owned(), "reserved name"));
    }
    if name
        .chars()
        .any(|c| RESERVED.contains(&c) || c == '\0' || c.is_control())
    {
        return Err(PlanError::BadName(
            name.to_owned(),
            "contains a character that is not allowed in file names",
        ));
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return Err(PlanError::BadName(
            name.to_owned(),
            "names cannot end with a dot or a space",
        ));
    }
    Ok(())
}

/// Renames `src` to `new_name` in the same folder.
///
/// Collision rules match copy: `Skip` leaves things alone, `Overwrite` replaces
/// a file (never a folder), `KeepBoth` picks a free `name (2)`, and `Rename`
/// fails when the name is taken.
pub fn rename_path(
    src: &Path,
    new_name: &str,
    collision: Collision,
) -> Result<RenameOutcome, Failure> {
    validate_name(new_name)
        .map_err(|error| Failure::new(src, FailureReason::InvalidName, error.to_string()))?;
    let parent = src
        .parent()
        .ok_or_else(|| Failure::new(src, FailureReason::Other, "the entry has no parent folder"))?;
    let source_meta = fs::symlink_metadata(src).map_err(|error| Failure::from_io(src, &error))?;
    let source_is_dir = is_dir_entry(&source_meta);
    if src.file_name().is_some_and(|old| old == new_name) {
        return Ok(RenameOutcome::Unchanged);
    }

    let mut target = parent.join(new_name);
    match fs::symlink_metadata(&target) {
        Ok(existing) => {
            // Case-only change on a case-insensitive volume: the same file, so no clash.
            if !same_location(src, &target) {
                match collision {
                    Collision::Skip => {
                        return Ok(RenameOutcome::Skipped { existing: target });
                    }
                    Collision::Rename => {
                        return Err(Failure::new(
                            &target,
                            FailureReason::Conflict,
                            "that name is taken; choose another name",
                        ));
                    }
                    Collision::KeepBoth => {
                        let free = free_name(parent, OsStr::new(new_name), source_is_dir)
                            .ok_or_else(|| {
                                Failure::new(&target, FailureReason::Conflict, "no free name found")
                            })?;
                        target = parent.join(free);
                    }
                    Collision::Overwrite => {
                        let existing_is_dir = is_dir_entry(&existing);
                        if source_is_dir || existing_is_dir || is_link_like(&existing) {
                            return Err(Failure::new(
                                &target,
                                FailureReason::Conflict,
                                "folders and links are never replaced; not renamed",
                            ));
                        }
                    }
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(Failure::from_io(&target, &error)),
    }

    fs::rename(src, &target).map_err(|error| Failure::from_io(src, &error))?;
    Ok(RenameOutcome::Renamed { to: target })
}
