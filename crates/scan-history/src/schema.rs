use std::path::PathBuf;

use crate::error::Error;
use crate::record::Scan;

/// The schema version this binary writes and reads natively.
pub const CURRENT_SCHEMA: u32 = 3;
/// The oldest schema version this binary can migrate in memory.
pub const OLDEST_MIGRATABLE_SCHEMA: u32 = 1;

/// Upgrades an older-schema scan to [`CURRENT_SCHEMA`] in memory.
///
/// A migrated scan is written back as [`CURRENT_SCHEMA`] the next time it is saved.
pub fn migrate(version: u32, scan: &mut Scan) -> Result<(), Error> {
    match version {
        CURRENT_SCHEMA => Ok(()),
        // Schema 1 recorded no file identity and schema 2 no Windows attributes. The decoder
        // leaves both `None` for those versions, so the only change is the version marker.
        OLDEST_MIGRATABLE_SCHEMA..=2 => {
            scan.schema_version = CURRENT_SCHEMA;
            Ok(())
        }
        found => Err(Error::UnsupportedMigration {
            path: PathBuf::new(),
            found,
            supported: CURRENT_SCHEMA,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provenance::{ScanMethod, ScanProvenance};
    use crate::record::ScanEntry;
    use std::path::PathBuf;

    fn v1_scan() -> Scan {
        Scan {
            provenance: ScanProvenance {
                method: ScanMethod::GenericTraversal,
                timestamp_unix_secs: 1_700_000_000,
                root: PathBuf::from("C:/root"),
            },
            entries: vec![ScanEntry {
                parent: None,
                name: OsString::from("root"),
                kind: timberstat_filesystem::EntryKind::Directory,
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
        }
    }

    use std::ffi::OsString;

    #[test]
    fn current_schema_passes_through() {
        let mut scan = v1_scan();
        scan.schema_version = CURRENT_SCHEMA;
        assert!(migrate(CURRENT_SCHEMA, &mut scan).is_ok());
        assert_eq!(scan.schema_version, CURRENT_SCHEMA);
    }

    #[test]
    fn v1_migrates_to_current() {
        let mut scan = v1_scan();
        assert!(migrate(1, &mut scan).is_ok());
        assert_eq!(scan.schema_version, CURRENT_SCHEMA);
        assert!(scan.entries[0].identity.is_none());
    }

    #[test]
    fn unknown_version_is_refused() {
        let mut scan = v1_scan();
        assert!(migrate(0, &mut scan).is_err());
        assert!(migrate(CURRENT_SCHEMA + 1, &mut scan).is_err());
        assert_eq!(scan.schema_version, 1);
    }
}
