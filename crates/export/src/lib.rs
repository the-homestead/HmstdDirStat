//! GPUI-free CSV and JSON exports from TimberStat's scan model.
//!
//! CSV is UTF-8 with RFC 4180 quoting and CRLF records. JSON documents carry
//! `schema_version` and scan provenance. JSON scan imports use the scan-history
//! record model, preserving its hierarchy, identity, and metadata fields.

mod csv;
mod document;
mod error;
mod row;
mod writer;

pub use csv::{duplicates_to_csv, scan_to_csv, search_to_csv};
pub use document::{
    ExportedScanMethod, Provenance, SCHEMA_VERSION, duplicates_to_json, import_scan_json,
    import_scan_json_file, scan_record_to_json, scan_to_json, search_to_json,
};
pub use error::ExportError;
pub use row::{ExportRow, ExportedEntryKind};
pub use writer::write_atomic;
