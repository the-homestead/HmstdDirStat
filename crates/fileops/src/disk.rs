//! Small filesystem helpers shared by delete, copy, move and rename.

use std::ffi::{OsStr, OsString};
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// A real directory: not a symlink, junction or other reparse point.
pub(crate) fn is_dir_entry(meta: &Metadata) -> bool {
    meta.file_type().is_dir()
}

/// A Windows directory symlink or junction. Removed with `remove_dir`, never followed.
pub(crate) fn is_link_to_dir(meta: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileTypeExt;
        meta.file_type().is_symlink_dir()
    }
    #[cfg(not(windows))]
    {
        let _ = meta;
        false
    }
}

/// A symlink or any Windows reparse point (junction, cloud placeholder, ...).
/// Copy and move never descend into these.
pub(crate) fn is_link_like(meta: &Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// True when both paths resolve to the same location on disk.
pub(crate) fn same_location(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// First free name in `dir` of the form `stem (2).ext`, `stem (3).ext`, ... Directories
/// get `name (2)`. `None` after 9999 tries.
pub(crate) fn free_name(dir: &Path, name: &OsStr, is_dir: bool) -> Option<OsString> {
    let as_path = Path::new(name);
    let (stem, extension) = match (as_path.file_stem(), as_path.extension()) {
        (Some(stem), Some(ext)) if !is_dir => (stem.to_os_string(), Some(ext.to_os_string())),
        _ => (name.to_os_string(), None),
    };
    for number in 2..=9999u32 {
        let mut candidate = stem.clone();
        candidate.push(format!(" ({number})"));
        if let Some(ext) = &extension {
            candidate.push(".");
            candidate.push(ext);
        }
        match std::fs::symlink_metadata(dir.join(&candidate)) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Some(candidate),
            _ => {}
        }
    }
    None
}

/// A unique temporary name beside `target`, used to stage a file before it
/// replaces the target. The name starts with a dot and carries a `.part` suffix.
pub(crate) fn staging_path(target: &Path, name: &OsStr) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nonce = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut staged = OsString::from(".");
    staged.push(name);
    staged.push(format!(".hmstd-{}-{nonce}.part", std::process::id()));
    target
        .parent()
        .map_or_else(|| PathBuf::from(&staged), |dir| dir.join(&staged))
}
