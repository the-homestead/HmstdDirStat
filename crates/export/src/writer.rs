use std::{
    fs::File,
    io::{self, Write},
    path::{Path, PathBuf},
};

use tempfile::NamedTempFile;

use crate::ExportError;

/// Writes a complete export to a same-directory temporary file, flushes it, then
/// atomically persists it at `path`. Existing files are replaced only when
/// `overwrite` is true. Serialization must finish before this function is called.
pub fn write_atomic(path: &Path, contents: &str, overwrite: bool) -> Result<(), ExportError> {
    write_inner(path, contents, overwrite, None)
}

/// Writes like [`write_atomic`], with one final caller check after the temporary
/// file is flushed and before it is persisted. A rejected check cleans up the
/// temporary file and preserves any existing destination.
pub fn write_atomic_with_check(
    path: &Path,
    contents: &str,
    overwrite: bool,
    before_persist: &dyn Fn() -> io::Result<()>,
) -> Result<(), ExportError> {
    write_inner(path, contents, overwrite, Some(before_persist))
}

fn write_inner(
    path: &Path,
    contents: &str,
    overwrite: bool,
    before_persist: Option<&dyn Fn() -> io::Result<()>>,
) -> Result<(), ExportError> {
    if !overwrite
        && path
            .try_exists()
            .map_err(|source| io_error("inspect", path, source))?
    {
        return Err(ExportError::AlreadyExists(path.to_path_buf()));
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temporary = NamedTempFile::new_in(parent)
        .map_err(|source| io_error("create a temporary export in", parent, source))?;
    temporary
        .write_all(contents.as_bytes())
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|source| io_error("write temporary export for", path, source))?;
    if let Some(hook) = before_persist {
        hook().map_err(|source| io_error("finish temporary export for", path, source))?;
    }
    persist(temporary, path, overwrite)
}

fn persist(
    temporary: NamedTempFile<File>,
    path: &Path,
    overwrite: bool,
) -> Result<(), ExportError> {
    let result = if overwrite {
        temporary.persist(path)
    } else {
        temporary.persist_noclobber(path)
    };
    result
        .map(|_| ())
        .map_err(|error| match error.error.kind() {
            io::ErrorKind::AlreadyExists if !overwrite => {
                ExportError::AlreadyExists(path.to_path_buf())
            }
            _ => io_error("persist export at", path, error.error),
        })
}

fn io_error(operation: &'static str, path: &Path, source: io::Error) -> ExportError {
    ExportError::Io {
        operation,
        path: PathBuf::from(path),
        source,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn interruption_before_rename_preserves_old_file_and_cleans_temp() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("scan.csv");
        fs::write(&path, "old complete export").unwrap();
        let interrupt = || Err(io::Error::other("injected interruption"));
        let error = write_inner(&path, "partial replacement", true, Some(&interrupt))
            .expect_err("the injected failure must be returned");
        assert!(matches!(error, ExportError::Io { .. }));
        assert_eq!(fs::read_to_string(&path).unwrap(), "old complete export");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn public_pre_persist_check_rejects_without_a_new_output_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("scan.json");
        let reject = || Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
        let error = write_atomic_with_check(&path, "partial", false, &reject)
            .expect_err("cancellation must abort before rename");
        assert!(matches!(error, ExportError::Io { .. }));
        assert!(!path.exists());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    }
}
