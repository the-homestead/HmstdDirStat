//! Versioned JSON persistence for saved workspaces. Every replacement goes through a sibling
//! temporary file, is flushed, and is then renamed. An existing file must itself load cleanly
//! before it can be replaced, so a damaged or newer file is never silently overwritten.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use crate::{config, error::ConfigError, workspace::WorkspaceLayout};

use super::{WORKSPACE_SCHEMA_VERSION, WORKSPACES_FILE_NAME, WorkspaceDocument};

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceStoreError {
    #[error("could not {operation} workspace file {path}: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("workspace file {path} is not valid JSON: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("workspace file {path} uses schema version {found}; this build supports {supported}")]
    UnsupportedVersion {
        path: PathBuf,
        found: u64,
        supported: u32,
    },
    #[error("workspace file {path} is inconsistent: {reason}")]
    Invalid { path: PathBuf, reason: String },
    #[error("workspace document could not be serialized for {path}: {source}")]
    Serialize {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

/// Resolves workspaces.json beside config.json without reading or writing either file.
pub fn default_workspaces_path() -> Result<PathBuf, ConfigError> {
    config::default_config_path().map(|path| path.with_file_name(WORKSPACES_FILE_NAME))
}

/// Loads a document, or starts from `fallback_layout` only when the file does not exist.
pub fn load_from(
    path: &Path,
    fallback_layout: WorkspaceLayout,
) -> Result<WorkspaceDocument, WorkspaceStoreError> {
    match read_existing(path) {
        Ok(document) => Ok(document),
        Err(WorkspaceStoreError::Io { source, .. })
            if source.kind() == std::io::ErrorKind::NotFound =>
        {
            Ok(WorkspaceDocument::new(fallback_layout))
        }
        Err(error) => Err(error),
    }
}

/// Atomically saves a valid current-schema document. A present destination is loaded and
/// validated first; corruption or an unknown schema aborts before a temporary file is created.
pub fn save_to(path: &Path, document: &WorkspaceDocument) -> Result<(), WorkspaceStoreError> {
    validate_for_write(path, document)?;
    if path
        .try_exists()
        .map_err(|source| io(path, "inspect", source))?
    {
        read_existing(path)?;
    }

    let mut json = serde_json::to_string_pretty(document).map_err(|source| {
        WorkspaceStoreError::Serialize {
            path: path.to_path_buf(),
            source,
        }
    })?;
    json.push('\n');
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| io(path, "create directory for", source))?;
    }

    let temporary = temporary_path(path);
    let result = (|| -> std::io::Result<()> {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)
    })();
    if let Err(source) = result {
        let _ = fs::remove_file(&temporary);
        return Err(io(path, "replace", source));
    }
    Ok(())
}

fn read_existing(path: &Path) -> Result<WorkspaceDocument, WorkspaceStoreError> {
    let text = fs::read_to_string(path).map_err(|source| io(path, "read", source))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|source| WorkspaceStoreError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
    if let Some(found) = value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        && found != u64::from(WORKSPACE_SCHEMA_VERSION)
    {
        return Err(WorkspaceStoreError::UnsupportedVersion {
            path: path.to_path_buf(),
            found,
            supported: WORKSPACE_SCHEMA_VERSION,
        });
    }
    let document: WorkspaceDocument =
        serde_json::from_value(value).map_err(|source| WorkspaceStoreError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
    document
        .validate()
        .map_err(|reason| WorkspaceStoreError::Invalid {
            path: path.to_path_buf(),
            reason,
        })?;
    Ok(document)
}

fn validate_for_write(
    path: &Path,
    document: &WorkspaceDocument,
) -> Result<(), WorkspaceStoreError> {
    if document.schema_version != WORKSPACE_SCHEMA_VERSION {
        return Err(WorkspaceStoreError::UnsupportedVersion {
            path: path.to_path_buf(),
            found: u64::from(document.schema_version),
            supported: WORKSPACE_SCHEMA_VERSION,
        });
    }
    document
        .validate()
        .map_err(|reason| WorkspaceStoreError::Invalid {
            path: path.to_path_buf(),
            reason,
        })
}

fn temporary_path(path: &Path) -> PathBuf {
    path.with_extension("json.tmp")
}

fn io(path: &Path, operation: &'static str, source: std::io::Error) -> WorkspaceStoreError {
    WorkspaceStoreError::Io {
        operation,
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{
        WORKSPACE_SCHEMA_VERSION, WorkspaceDocument, WorkspaceStoreError, load_from, save_to,
        temporary_path,
    };
    use crate::workspace::{WidgetKind, WorkspaceLayout};

    fn document() -> WorkspaceDocument {
        let mut document = WorkspaceDocument::default();
        document.switch("Deep directory analysis").unwrap();
        document.layout.hide(WidgetKind::Tree);
        document.save_current_as("Custom layout").unwrap();
        document
    }

    #[test]
    fn file_round_trip_preserves_custom_workspaces_and_active_layout() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested").join("workspaces.json");
        let document = document();
        save_to(&path, &document).unwrap();
        let loaded = load_from(&path, WorkspaceLayout::default()).unwrap();
        assert_eq!(loaded, document);
        assert!(!temporary_path(&path).exists());
    }

    #[test]
    fn missing_file_uses_the_fallback_without_creating_a_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("workspaces.json");
        let fallback = WorkspaceLayout::default();
        let loaded = load_from(&path, fallback.clone()).unwrap();
        assert_eq!(loaded.layout, fallback);
        assert!(!path.exists());
    }

    #[test]
    fn corrupt_file_is_reported_and_cannot_be_overwritten_by_save() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("workspaces.json");
        fs::write(&path, "{ not json").unwrap();
        assert!(matches!(
            load_from(&path, WorkspaceLayout::default()),
            Err(WorkspaceStoreError::Parse { .. })
        ));
        assert!(matches!(
            save_to(&path, &WorkspaceDocument::default()),
            Err(WorkspaceStoreError::Parse { .. })
        ));
        assert_eq!(fs::read_to_string(path).unwrap(), "{ not json");
    }

    #[test]
    fn unknown_version_is_reported_and_never_replaced() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("workspaces.json");
        let original = r#"{"schema_version":99,"active":"future"}"#;
        fs::write(&path, original).unwrap();
        for result in [
            load_from(&path, WorkspaceLayout::default()),
            save_to(&path, &WorkspaceDocument::default()).map(|()| WorkspaceDocument::default()),
        ] {
            match result {
                Err(WorkspaceStoreError::UnsupportedVersion {
                    found, supported, ..
                }) => {
                    assert_eq!(found, 99);
                    assert_eq!(supported, WORKSPACE_SCHEMA_VERSION);
                }
                other => panic!("expected unsupported version, got {other:?}"),
            }
        }
        assert_eq!(fs::read_to_string(path).unwrap(), original);
    }

    #[test]
    fn atomic_save_replaces_a_valid_file_and_cleans_up_its_temporary_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("workspaces.json");
        save_to(&path, &WorkspaceDocument::default()).unwrap();
        let replacement = document();
        save_to(&path, &replacement).unwrap();
        assert_eq!(
            load_from(&path, WorkspaceLayout::default()).unwrap(),
            replacement
        );
        assert!(!temporary_path(&path).exists());
    }

    #[test]
    fn failed_atomic_write_leaves_the_previous_file_byte_for_byte_intact() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("workspaces.json");
        save_to(&path, &WorkspaceDocument::default()).unwrap();
        let before = fs::read(&path).unwrap();
        // A directory at the temporary path forces File::create to fail before rename.
        fs::create_dir(temporary_path(&path)).unwrap();
        assert!(save_to(&path, &document()).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        fs::remove_dir(temporary_path(&path)).unwrap();
    }
}
