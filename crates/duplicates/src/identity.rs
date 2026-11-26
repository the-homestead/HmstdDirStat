//! Hard-link identity. Two paths with equal keys name the same storage object.

use std::io;
use std::path::Path;

use same_file::Handle;

/// Key of the storage object behind `path`, from the platform's file key
/// (volume and file index on Windows, device and inode on Unix).
///
/// Opens the file read-only. A file that cannot be opened is unreadable, so
/// the error also means the file cannot be compared.
pub fn file_key(path: &Path) -> io::Result<Handle> {
    Handle::from_path(path)
}
