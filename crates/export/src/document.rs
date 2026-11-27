use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use timberstat_duplicates::DuplicateReport;
use timberstat_filesystem::{FileIdentity, FileTree};
use timberstat_scan_history::{Scan, ScanEntry, ScanMethod, ScanProvenance};
use timberstat_search::Evaluation;

use crate::{
    ExportError, ExportRow,
    row::{row_from_tree, tree_hard_link_counts, unicode_path},
};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportedScanMethod {
    GenericTraversal,
    NtfsMft,
}

/// JSON provenance header. The timestamp is seconds since the Unix epoch, using
/// the same representation as the scan-history record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub root: String,
    pub scan_method: ExportedScanMethod,
    pub timestamp_unix_secs: i64,
}

impl Provenance {
    pub fn from_history(provenance: &ScanProvenance) -> Result<Self, ExportError> {
        Ok(Self {
            root: unicode_path(provenance.root.clone())?,
            scan_method: match provenance.method {
                ScanMethod::GenericTraversal => ExportedScanMethod::GenericTraversal,
                ScanMethod::NtfsMft => ExportedScanMethod::NtfsMft,
            },
            timestamp_unix_secs: provenance.timestamp_unix_secs,
        })
    }

    fn to_history(&self) -> ScanProvenance {
        ScanProvenance {
            method: match self.scan_method {
                ExportedScanMethod::GenericTraversal => ScanMethod::GenericTraversal,
                ExportedScanMethod::NtfsMft => ScanMethod::NtfsMft,
            },
            timestamp_unix_secs: self.timestamp_unix_secs,
            root: PathBuf::from(&self.root),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ScanDocument {
    schema_version: u32,
    provenance: Provenance,
    entries: Vec<ScanJsonEntry>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct ScanJsonEntry {
    #[serde(flatten)]
    row: ExportRow,
    parent: Option<u32>,
    identity: Option<IdentityDocument>,
    hardlink_duplicate: bool,
    unreadable: bool,
    readonly: bool,
    windows_attributes: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct IdentityDocument {
    volume: u64,
    file_id: u64,
}

impl From<FileIdentity> for IdentityDocument {
    fn from(identity: FileIdentity) -> Self {
        Self {
            volume: identity.volume,
            file_id: identity.file_id,
        }
    }
}

impl From<IdentityDocument> for FileIdentity {
    fn from(identity: IdentityDocument) -> Self {
        Self {
            volume: identity.volume,
            file_id: identity.file_id,
        }
    }
}

#[derive(Debug, Serialize)]
struct SearchDocument {
    schema_version: u32,
    provenance: Provenance,
    query: String,
    results: Vec<ExportRow>,
}

#[derive(Debug, Serialize)]
struct DuplicatesDocument {
    schema_version: u32,
    provenance: Provenance,
    groups: Vec<DuplicateGroupDocument>,
}

#[derive(Debug, Serialize)]
struct DuplicateGroupDocument {
    id: u64,
    size: u64,
    digest: String,
    reclaimable_size: u64,
    members: Vec<ExportRow>,
}

/// Converts a live scan model to the scan-history record and exports it as JSON.
pub fn scan_to_json(tree: &FileTree, provenance: &ScanProvenance) -> Result<String, ExportError> {
    let scan = Scan::from_tree(provenance.clone(), tree).map_err(ExportError::ScanModel)?;
    scan_record_to_json(&scan)
}

/// Exports an existing scan-history record without changing its entry order.
pub fn scan_record_to_json(scan: &Scan) -> Result<String, ExportError> {
    encode(&ScanDocument {
        schema_version: SCHEMA_VERSION,
        provenance: Provenance::from_history(&scan.provenance)?,
        entries: scan_entries(scan)?,
    })
}

pub fn search_to_json(
    tree: &FileTree,
    results: &Evaluation,
    query: &str,
    provenance: &ScanProvenance,
) -> Result<String, ExportError> {
    let hard_links = tree_hard_link_counts(tree);
    let mut rows = Vec::with_capacity(results.ids.len());
    for &id in &results.ids {
        rows.push(row_from_tree(tree, id, &hard_links)?);
    }
    encode(&SearchDocument {
        schema_version: SCHEMA_VERSION,
        provenance: Provenance::from_history(provenance)?,
        query: query.to_owned(),
        results: rows,
    })
}

pub fn duplicates_to_json(
    tree: &FileTree,
    report: &DuplicateReport,
    provenance: &ScanProvenance,
) -> Result<String, ExportError> {
    let hard_links = tree_hard_link_counts(tree);
    let mut groups = Vec::with_capacity(report.groups.len());
    for (group_index, group) in report.groups.iter().enumerate() {
        let mut members = Vec::with_capacity(group.members.len());
        for member in &group.members {
            members.push(row_from_tree(tree, member.id, &hard_links)?);
        }
        groups.push(DuplicateGroupDocument {
            id: u64::try_from(group_index)
                .unwrap_or(u64::MAX)
                .saturating_add(1),
            size: group.size,
            digest: group.digest_hex(),
            reclaimable_size: group.reclaimable(),
            members,
        });
    }
    encode(&DuplicatesDocument {
        schema_version: SCHEMA_VERSION,
        provenance: Provenance::from_history(provenance)?,
        groups,
    })
}

/// Imports a JSON scan into the same record type persisted by scan-history.
/// Derived path, depth, and hard-link columns are validated rather than trusted.
pub fn import_scan_json(json: &str) -> Result<Scan, ExportError> {
    let document: ScanDocument = serde_json::from_str(json).map_err(ExportError::JsonDecode)?;
    if document.schema_version != SCHEMA_VERSION {
        return Err(ExportError::UnsupportedSchema {
            found: document.schema_version,
            supported: SCHEMA_VERSION,
        });
    }
    let entries = document
        .entries
        .iter()
        .map(|entry| ScanEntry {
            parent: entry.parent,
            name: entry.row.name.clone().into(),
            kind: entry.row.kind.into(),
            logical_size: entry.row.logical_size,
            physical_size: entry.row.allocated_size,
            identity: entry.identity.map(FileIdentity::from),
            hardlink_duplicate: entry.hardlink_duplicate,
            unreadable: entry.unreadable,
            modified_unix_secs: entry.row.modified,
            readonly: entry.readonly,
            windows_attributes: entry.windows_attributes,
        })
        .collect();
    let scan = Scan::new(document.provenance.to_history(), entries);
    let rebuilt = scan_entries(&scan)?;
    if rebuilt != document.entries {
        let index = rebuilt
            .iter()
            .zip(&document.entries)
            .position(|(left, right)| left != right)
            .unwrap_or_else(|| rebuilt.len().min(document.entries.len()));
        return Err(ExportError::InvalidScan(format!(
            "entry {index} has inconsistent path, depth, hierarchy, or hard-link data"
        )));
    }
    Ok(scan)
}

pub fn import_scan_json_file(path: &Path) -> Result<Scan, ExportError> {
    let json = fs::read_to_string(path).map_err(|source| ExportError::Io {
        operation: "read JSON export",
        path: path.to_path_buf(),
        source,
    })?;
    import_scan_json(&json)
}

fn encode(document: &impl Serialize) -> Result<String, ExportError> {
    let mut json = serde_json::to_string_pretty(document).map_err(ExportError::JsonEncode)?;
    json.push('\n');
    Ok(json)
}

fn scan_entries(scan: &Scan) -> Result<Vec<ScanJsonEntry>, ExportError> {
    validate_hierarchy(scan)?;
    let mut hard_links = HashMap::new();
    for entry in &scan.entries {
        if let Some(identity) = entry.identity {
            *hard_links.entry(identity).or_insert(0_u64) += 1;
        }
    }
    let mut exported = Vec::with_capacity(scan.len());
    for (index, entry) in scan.entries.iter().enumerate() {
        let index = u32::try_from(index)
            .map_err(|_| ExportError::InvalidScan("entry index exceeds u32".to_owned()))?;
        let path = scan
            .path_of(index)
            .ok_or_else(|| ExportError::InvalidScan(format!("entry {index} has no path")))?;
        let name = entry
            .name
            .clone()
            .into_string()
            .map_err(|name| ExportError::NonUnicodePath(PathBuf::from(name)))?;
        let mut depth = 0_u32;
        let mut parent = entry.parent;
        while let Some(parent_index) = parent {
            depth = depth.saturating_add(1);
            parent = scan.entries[parent_index as usize].parent;
        }
        exported.push(ScanJsonEntry {
            row: ExportRow {
                path: unicode_path(path)?,
                name,
                kind: entry.kind.into(),
                logical_size: entry.logical_size,
                allocated_size: entry.physical_size,
                modified: entry.modified_unix_secs,
                depth,
                hard_link_count: entry
                    .identity
                    .and_then(|identity| hard_links.get(&identity).copied()),
            },
            parent: entry.parent,
            identity: entry.identity.map(IdentityDocument::from),
            hardlink_duplicate: entry.hardlink_duplicate,
            unreadable: entry.unreadable,
            readonly: entry.readonly,
            windows_attributes: entry.windows_attributes,
        });
    }
    Ok(exported)
}

fn validate_hierarchy(scan: &Scan) -> Result<(), ExportError> {
    if scan.is_empty() {
        return Err(ExportError::InvalidScan(
            "a scan must contain its root entry".to_owned(),
        ));
    }
    if scan.entries[0].parent.is_some() {
        return Err(ExportError::InvalidScan(
            "entry 0 must be the root".to_owned(),
        ));
    }
    for (index, entry) in scan.entries.iter().enumerate().skip(1) {
        let Some(parent) = entry.parent else {
            return Err(ExportError::InvalidScan(format!(
                "entry {index} is a second root"
            )));
        };
        if parent as usize >= index {
            return Err(ExportError::InvalidScan(format!(
                "entry {index} has invalid parent {parent}"
            )));
        }
    }
    Ok(())
}
