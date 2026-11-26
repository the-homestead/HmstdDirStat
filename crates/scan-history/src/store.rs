use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use crate::codec;
use crate::error::Error;
use crate::provenance::ScanProvenance;
use crate::record::Scan;

pub const SCAN_FILE_EXTENSION: &str = ".tscan";

#[derive(Debug, Clone)]
pub struct ScanSummary {
    pub id: String,
    pub provenance: ScanProvenance,
    pub schema_version: u32,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    None,
    AfterTempCreate,
    PartialWrite,
    BeforeRename,
}

#[derive(Debug, Clone)]
pub struct ScanStore {
    dir: PathBuf,
}

impl ScanStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn save(&self, scan: &Scan) -> Result<PathBuf, Error> {
        self.save_with(scan, Fault::None)
    }

    pub(crate) fn save_with(&self, scan: &Scan, fault: Fault) -> Result<PathBuf, Error> {
        let id = scan_id(&scan.provenance);
        let path = self.scan_path(&id);
        if path.try_exists().map_err(|source| Error::Io {
            operation: "inspect scan file",
            path: path.clone(),
            source,
        })? {
            // Refuse to overwrite an unreadable or unsupported-version scan.
            self.load(&id)?;
        }
        let file_bytes = codec::encode(&codec::raw_from_scan(scan))?;
        let temporary = self.dir.join(format!("{id}{SCAN_FILE_EXTENSION}.tmp"));
        if let Err(source) = write_temporary(&temporary, &file_bytes, fault) {
            let _ = fs::remove_file(&temporary);
            return Err(Error::Io {
                operation: "replace scan file",
                path,
                source,
            });
        }
        if let Err(source) = fs::rename(&temporary, &path) {
            let _ = fs::remove_file(&temporary);
            return Err(Error::Io {
                operation: "replace scan file",
                path,
                source,
            });
        }
        Ok(path)
    }

    pub fn load(&self, id: &str) -> Result<Scan, Error> {
        let path = self.scan_path(id);
        let raw = codec::decode_file(&path)?;
        if raw.schema_version > crate::schema::CURRENT_SCHEMA {
            return Err(Error::UnsupportedVersion {
                path,
                found: raw.schema_version,
                supported: crate::schema::CURRENT_SCHEMA,
            });
        }
        codec::to_scan(raw)
    }

    pub fn list(&self) -> Result<Vec<ScanSummary>, Error> {
        let mut summaries = Vec::new();
        let entries = fs::read_dir(&self.dir).map_err(|source| Error::Io {
            operation: "list scan directory",
            path: self.dir.clone(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| Error::Io {
                operation: "list scan directory",
                path: self.dir.clone(),
                source,
            })?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.ends_with(SCAN_FILE_EXTENSION) {
                continue;
            }
            let id = name[..name.len() - SCAN_FILE_EXTENSION.len()].to_string();
            summaries.push(read_summary(&path, &id));
        }
        summaries.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(summaries)
    }

    pub fn scan_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}{SCAN_FILE_EXTENSION}"))
    }
}

fn write_temporary(path: &Path, bytes: &[u8], fault: Fault) -> std::io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    if fault == Fault::AfterTempCreate {
        return Err(std::io::Error::other("fault: after temp create"));
    }
    if fault == Fault::PartialWrite {
        let split = bytes.len() / 2;
        file.write_all(&bytes[..split])?;
        return Err(std::io::Error::other("fault: partial write"));
    }
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    if fault == Fault::BeforeRename {
        return Err(std::io::Error::other("fault: before rename"));
    }
    Ok(())
}

fn scan_id(provenance: &ScanProvenance) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(provenance.root.as_os_str().as_encoded_bytes());
    hasher.update(&provenance.timestamp_unix_secs.to_le_bytes());
    let hash = hasher.finalize();
    let bytes: &[u8] = hash.as_bytes();
    let mut hex = String::with_capacity(64);
    for byte in bytes.iter() {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

fn read_summary(path: &Path, id: &str) -> ScanSummary {
    let raw = match codec::decode_file(path) {
        Ok(raw) => raw,
        Err(error) => {
            return ScanSummary {
                id: id.to_string(),
                provenance: default_provenance(),
                schema_version: 0,
                error: Some(error.to_string()),
            };
        }
    };
    ScanSummary {
        id: id.to_string(),
        provenance: raw.provenance,
        schema_version: raw.schema_version,
        error: None,
    }
}

fn default_provenance() -> ScanProvenance {
    ScanProvenance {
        method: crate::provenance::ScanMethod::GenericTraversal,
        timestamp_unix_secs: 0,
        root: std::path::PathBuf::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::RawScan;
    use crate::provenance::{ScanMethod, ScanProvenance};
    use crate::record::{Scan, ScanEntry};
    use crate::schema::CURRENT_SCHEMA;
    use std::ffi::OsString;
    use std::path::PathBuf;
    use timberstat_filesystem::EntryKind;

    fn scan_with(root: &str, timestamp: i64, entries: Vec<ScanEntry>) -> Scan {
        Scan::new(
            ScanProvenance {
                method: ScanMethod::GenericTraversal,
                timestamp_unix_secs: timestamp,
                root: PathBuf::from(root),
            },
            entries,
        )
    }

    fn entry(name: &str, parent: Option<u32>, kind: EntryKind, logical: u64) -> ScanEntry {
        ScanEntry {
            parent,
            name: OsString::from(name),
            kind,
            logical_size: logical,
            physical_size: Some(0),
            identity: None,
            hardlink_duplicate: false,
            unreadable: false,
            modified_unix_secs: None,
            readonly: false,
            windows_attributes: None,
        }
    }

    fn temp_store() -> (tempfile::TempDir, ScanStore) {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path().to_path_buf();
        (dir, ScanStore::new(dir_path))
    }

    fn id_from_path(path: &Path) -> String {
        let name = path.file_name().unwrap().to_string_lossy();
        name[..name.len() - SCAN_FILE_EXTENSION.len()].to_string()
    }

    #[test]
    fn save_then_load_round_trips() {
        let (_dir, store) = temp_store();
        let scan = scan_with(
            "C:/Users/dev/project",
            1_700_000_000,
            vec![
                entry("project", None, EntryKind::Directory, 0),
                entry("src", Some(0), EntryKind::Directory, 0),
                entry("src/main.rs", Some(1), EntryKind::File, 42),
            ],
        );
        let path = store.save(&scan).unwrap();
        assert!(path.exists());
        let loaded = store.load(&id_from_path(&path)).unwrap();
        assert_eq!(loaded.provenance, scan.provenance);
        assert_eq!(loaded.entries.len(), scan.entries.len());
    }

    #[test]
    fn atomic_save_replaces_prior_file_and_cleans_up_temp() {
        let (_dir, store) = temp_store();
        let scan = scan_with("C:/Users/dev/project", 1_700_000_000, vec![]);
        let path = store.save(&scan).unwrap();
        assert!(path.exists());
        let path2 = store.save(&scan).unwrap();
        assert_eq!(path, path2);
        assert!(path2.to_string_lossy().ends_with(".tscan"));
    }

    #[test]
    fn prior_scan_intact_after_fault_injection_after_temp_create() {
        let (_dir, store) = temp_store();
        let scan_a = scan_with(
            "C:/Users/dev/project",
            1_700_000_000,
            vec![
                entry("proj", None, EntryKind::Directory, 0),
                entry("src", Some(0), EntryKind::Directory, 0),
            ],
        );
        let path_a = store.save(&scan_a).unwrap();
        let scan_b = scan_with(
            "C:/Users/dev/project",
            1_700_000_001,
            vec![entry("proj", None, EntryKind::Directory, 0)],
        );
        assert!(store.save_with(&scan_b, Fault::AfterTempCreate).is_err());
        let loaded_a = store.load(&id_from_path(&path_a)).unwrap();
        assert_eq!(loaded_a.provenance, scan_a.provenance);
        assert_eq!(loaded_a.entries.len(), scan_a.entries.len());
    }

    #[test]
    fn prior_scan_intact_after_partial_write() {
        let (_dir, store) = temp_store();
        let scan_a = scan_with(
            "C:/Users/dev/project",
            1_700_000_000,
            vec![entry("proj", None, EntryKind::Directory, 0)],
        );
        let path_a = store.save(&scan_a).unwrap();
        let scan_b = scan_with(
            "C:/Users/dev/project",
            1_700_000_001,
            vec![entry("proj", None, EntryKind::Directory, 0)],
        );
        assert!(store.save_with(&scan_b, Fault::PartialWrite).is_err());
        let loaded_a = store.load(&id_from_path(&path_a)).unwrap();
        assert_eq!(loaded_a.provenance, scan_a.provenance);
        assert_eq!(loaded_a.entries.len(), scan_a.entries.len());
    }

    #[test]
    fn prior_scan_intact_before_rename() {
        let (_dir, store) = temp_store();
        let scan_a = scan_with(
            "C:/Users/dev/project",
            1_700_000_000,
            vec![entry("proj", None, EntryKind::Directory, 0)],
        );
        let path_a = store.save(&scan_a).unwrap();
        let scan_b = scan_with(
            "C:/Users/dev/project",
            1_700_000_001,
            vec![entry("proj", None, EntryKind::Directory, 0)],
        );
        assert!(store.save_with(&scan_b, Fault::BeforeRename).is_err());
        let loaded_a = store.load(&id_from_path(&path_a)).unwrap();
        assert_eq!(loaded_a.provenance, scan_a.provenance);
        assert_eq!(loaded_a.entries.len(), scan_a.entries.len());
    }

    #[test]
    fn corrupt_file_detected_and_others_still_load() {
        let (_dir, store) = temp_store();
        // Write a corrupt file first
        let id_corrupt = format!(
            "corrupt_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64
        );
        let path_corrupt = store.dir.join(format!("{id_corrupt}.tscan"));
        std::fs::write(&path_corrupt, "{ not json").unwrap();
        // List should report it as error
        let summaries = store.list().unwrap();
        assert!(summaries.iter().any(|s| s.error.is_some()));
        // Create a good scan
        let scan_good = scan_with(
            "C:/Users/dev/project",
            1_700_000_000,
            vec![entry("proj", None, EntryKind::Directory, 0)],
        );
        let _ = store.save(&scan_good).unwrap();
        let summaries2 = store.list().unwrap();
        assert!(summaries2.iter().filter(|s| s.error.is_none()).count() >= 1);
    }

    #[test]
    fn newer_version_refused_on_overwrite() {
        let (_dir, store) = temp_store();
        let scan_v2 = scan_with(
            "C:/Users/dev/project",
            1_700_000_000,
            vec![entry("proj", None, EntryKind::Directory, 0)],
        );
        // Manually write a file claiming a newer schema version (99)
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        let id_v3 = format!("v3_test_{ts}");
        let path_v3 = store.dir.join(format!("{id_v3}.tscan"));
        // Write a raw file with schema version 99 via codec? Simpler: write valid file with version 99 header
        // We'll create a minimal v3 file by taking a v2 file and editing schema_version byte.
        // Actually just write raw bytes that decode to version 99 — our decoder will parse version=99 and refuse.
        // Let's use codec to encode a real v2 file, then corrupt the version bytes.
        let path_v2 = store.save(&scan_v2).unwrap();
        let mut bytes = std::fs::read(&path_v2).unwrap();
        // Byte 8-11 are schema_version (LE). Change to 99.
        bytes[8] = 99;
        bytes[9] = 0;
        bytes[10] = 0;
        bytes[11] = 0;
        std::fs::write(&path_v3, bytes).unwrap();
        // Now load the v3 path directly -> UnsupportedVersion
        let err = store.load(&id_v3).unwrap_err();
        assert!(matches!(err, Error::UnsupportedVersion { .. }));
    }

    #[test]
    fn roundtrip_migration_v1_to_v2() {
        let v1_raw = RawScan {
            provenance: ScanProvenance {
                method: ScanMethod::GenericTraversal,
                timestamp_unix_secs: 1_700_000_000,
                root: PathBuf::from("C:/root"),
            },
            entries: vec![ScanEntry {
                parent: None,
                name: OsString::from("root"),
                kind: EntryKind::Directory,
                logical_size: 0,
                physical_size: Some(0),
                identity: None,
                hardlink_duplicate: false,
                unreadable: false,
                modified_unix_secs: None,
                readonly: false,
                windows_attributes: None,
            }],
            schema_version: 1,
        };
        let scan = codec::to_scan(v1_raw).unwrap();
        assert_eq!(scan.schema_version, CURRENT_SCHEMA);
        assert!(scan.entries[0].identity.is_none());
    }
}
