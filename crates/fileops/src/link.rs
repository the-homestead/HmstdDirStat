//! Creates symbolic links and Windows shortcut (`.lnk`) files.
//!
//! Neither kind ever replaces an existing entry. The name is claimed with
//! an exclusive create, so a clash can only be reported, never overwritten.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use crate::disk::free_name;
use crate::model::{Failure, FailureReason};
use crate::transfer_plan::Collision;

/// Result of a link creation that did not fail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkOutcome {
    /// The link now exists at this path.
    Created { path: PathBuf },
    /// The name was taken and the policy was skip. Nothing changed.
    Skipped { existing: PathBuf },
}

/// Creates a symbolic link at `link` that points at `target`.
///
/// `target` must exist and must be absolute, so the link keeps working when
/// it is moved. `link` must not exist. `Skip` and `KeepBoth` pick the
/// existing or a free name instead of failing. `Overwrite` is refused: links
/// are never replaced.
///
/// On Windows, creating a symbolic link needs Developer Mode or
/// administrator rights. Without them the failure is `AccessDenied` with a
/// message that says so.
pub fn create_symlink(
    target: &Path,
    link: &Path,
    collision: Collision,
) -> Result<LinkOutcome, Failure> {
    let target_meta = check_target(target)?;
    let target_is_dir = target_meta.is_dir();
    let final_path = match resolve_link_name(link, collision, target_is_dir)? {
        Resolved::Free(path) => path,
        Resolved::Skip(existing) => return Ok(LinkOutcome::Skipped { existing }),
    };

    let made = make_symlink(target, &final_path, target_is_dir);
    match made {
        Ok(()) => Ok(LinkOutcome::Created { path: final_path }),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Err(Failure::new(
            &final_path,
            FailureReason::Conflict,
            "a file with that name appeared while creating the link; nothing was replaced",
        )),
        Err(error) if error.raw_os_error() == Some(1314) => Err(Failure::new(
            &final_path,
            FailureReason::AccessDenied,
            "Windows needs Developer Mode or administrator rights to create symbolic links",
        )),
        Err(error) => Err(Failure::from_io(&final_path, &error)),
    }
}

/// Checks the target exists and is absolute. Returns its metadata.
pub(crate) fn check_target(target: &Path) -> Result<fs::Metadata, Failure> {
    if !target.is_absolute() {
        return Err(Failure::new(
            target,
            FailureReason::Other,
            "a link target must be an absolute path",
        ));
    }
    fs::metadata(target).map_err(|error| Failure::from_io(target, &error))
}

/// What the requested name resolves to after the collision policy.
pub(crate) enum Resolved {
    Free(PathBuf),
    Skip(PathBuf),
}

/// Picks the final name for a new link. Returns `Free` with a path nothing
/// occupies yet, or `Skip` when the policy keeps the existing entry.
pub(crate) fn resolve_link_name(
    link: &Path,
    collision: Collision,
    is_dir: bool,
) -> Result<Resolved, Failure> {
    let parent = link
        .parent()
        .ok_or_else(|| Failure::new(link, FailureReason::Other, "the link has no parent folder"))?;
    let name = link.file_name().ok_or_else(|| {
        Failure::new(
            link,
            FailureReason::InvalidName,
            "the link has no file name",
        )
    })?;
    match fs::symlink_metadata(link) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Resolved::Free(link.to_path_buf()));
        }
        Err(error) => return Err(Failure::from_io(link, &error)),
        Ok(_) => {}
    }
    match collision {
        Collision::Skip => Ok(Resolved::Skip(link.to_path_buf())),
        Collision::KeepBoth => {
            let free: OsString = free_name(parent, name, is_dir)
                .ok_or_else(|| Failure::new(link, FailureReason::Conflict, "no free name found"))?;
            Ok(Resolved::Free(parent.join(free)))
        }
        Collision::Rename => Err(Failure::new(
            link,
            FailureReason::Conflict,
            "that name is taken; choose another name",
        )),
        Collision::Overwrite => Err(Failure::new(
            link,
            FailureReason::Conflict,
            "links are never replaced; nothing was created",
        )),
    }
}

#[cfg(unix)]
fn make_symlink(target: &Path, link: &Path, _target_is_dir: bool) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn make_symlink(target: &Path, link: &Path, target_is_dir: bool) -> std::io::Result<()> {
    if target_is_dir {
        std::os::windows::fs::symlink_dir(target, link)
    } else {
        std::os::windows::fs::symlink_file(target, link)
    }
}
