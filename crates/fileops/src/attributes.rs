//! Change the read-only and hidden attributes of one entry.
//!
//! Links and junctions are refused: a change through them would land on the
//! target, which is outside the selection. Read-only uses `std` on every
//! platform. Hidden exists only on Windows and is set with `attrib.exe`,
//! because this crate forbids `unsafe` and the Win32 call needs it. Every
//! change is read back, so a success report means the disk says so.

use std::fs;
use std::path::Path;

use crate::disk::is_link_like;
use crate::model::{Failure, FailureReason};

/// Which attributes to change. `None` leaves that attribute as it is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AttributeChange {
    pub readonly: Option<bool>,
    pub hidden: Option<bool>,
}

impl AttributeChange {
    pub fn is_empty(self) -> bool {
        self.readonly.is_none() && self.hidden.is_none()
    }
}

/// Attributes as read back from the disk after a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attributes {
    pub readonly: bool,
    /// Always `false` off Windows, where the attribute does not exist.
    pub hidden: bool,
}

/// Applies `change` to one file or folder and returns what the disk reports.
///
/// Refusals (returned as [`Failure`], nothing changed):
/// - the path is not absolute,
/// - the entry is a symlink, junction or other reparse point,
/// - `hidden` is asked for off Windows.
///
/// If the read-only change succeeds and the hidden change then fails, the
/// read-only change stays. The failure detail says so.
pub fn set_attributes(path: &Path, change: AttributeChange) -> Result<Attributes, Failure> {
    if !path.is_absolute() {
        return Err(Failure::new(
            path,
            FailureReason::Other,
            "attribute changes need an absolute path",
        ));
    }
    if change.is_empty() {
        return read_attributes(path);
    }
    let meta = fs::symlink_metadata(path).map_err(|error| Failure::from_io(path, &error))?;
    if is_link_like(&meta) {
        return Err(Failure::new(
            path,
            FailureReason::Other,
            "links and junctions are not changed; their targets are left alone",
        ));
    }
    if change.hidden.is_some() && !cfg!(windows) {
        return Err(Failure::new(
            path,
            FailureReason::Other,
            "the hidden attribute exists only on Windows",
        ));
    }

    if let Some(readonly) = change.readonly {
        let mut permissions = meta.permissions();
        permissions.set_readonly(readonly);
        fs::set_permissions(path, permissions).map_err(|error| Failure::from_io(path, &error))?;
    }
    if let Some(hidden) = change.hidden {
        set_hidden(path, hidden).map_err(|failure| {
            if change.readonly.is_some() {
                Failure::new(
                    path,
                    failure.reason,
                    format!(
                        "{}; the read-only change was already applied",
                        failure.detail
                    ),
                )
            } else {
                failure
            }
        })?;
    }

    let after = read_attributes(path)?;
    let mismatch = change.readonly.is_some_and(|want| want != after.readonly)
        || change.hidden.is_some_and(|want| want != after.hidden);
    if mismatch {
        return Err(Failure::new(
            path,
            FailureReason::AccessDenied,
            "the disk did not keep the requested attributes",
        ));
    }
    Ok(after)
}

/// Reads the attributes the disk reports now. Follows no links.
pub fn read_attributes(path: &Path) -> Result<Attributes, Failure> {
    let meta = fs::symlink_metadata(path).map_err(|error| Failure::from_io(path, &error))?;
    Ok(Attributes {
        readonly: meta.permissions().readonly(),
        hidden: windows_hidden_bit(&meta),
    })
}

#[cfg(windows)]
fn windows_hidden_bit(meta: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x0000_0002;
    meta.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0
}

#[cfg(not(windows))]
fn windows_hidden_bit(_meta: &fs::Metadata) -> bool {
    false
}

#[cfg(windows)]
fn set_hidden(path: &Path, hidden: bool) -> Result<(), Failure> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let system_root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    let attrib = std::path::PathBuf::from(system_root)
        .join("System32")
        .join("attrib.exe");
    let flag = if hidden { "+H" } else { "-H" };
    let output = Command::new(attrib)
        .arg(flag)
        .arg(path)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|error| Failure::from_io(path, &error))?;
    if output.status.success() {
        return Ok(());
    }
    let mut detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if detail.is_empty() {
        detail = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    }
    if detail.is_empty() {
        detail = format!("attrib.exe exited with {}", output.status);
    }
    Err(Failure::new(path, FailureReason::Other, detail))
}

#[cfg(not(windows))]
fn set_hidden(path: &Path, _hidden: bool) -> Result<(), Failure> {
    Err(Failure::new(
        path,
        FailureReason::Other,
        "the hidden attribute exists only on Windows",
    ))
}
