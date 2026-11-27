//! Versioned, atomic persistence of filesystem scans for TimberStat.
//!
//! # Features
//!
//! - **Save, list, and reopen scans**, each with a version number and scan provenance
//!   (scan method, timestamp, root path).
//! - **Versioned schema**: a header with `schema_version` and a BLAKE3 checksum.
//!   An unknown or newer version is refused, not overwritten.
//! - **Atomic save**: write to a temp file, flush, then rename. The earlier committed
//!   scan remains intact if the save is interrupted at any step.
//! - **Compact encoding**: varint-packed integers, varint-byte strings, and a
//!   zigzag-encoded timestamp suitable for millions of entries.
//! - **Corrupt-file detection**: every file includes a BLAKE3 checksum; loading a
//!   corrupted file reports an error while other scans keep loading.
//! - **Migration hook**: [`crate::migrate`] upgrades an older schema version to the
//!   current one in memory.

mod codec;
mod error;
mod provenance;
mod rebuild;
mod record;
mod schema;
mod store;

pub use crate::provenance::{ScanMethod, ScanProvenance};
pub use crate::record::{Scan, ScanEntry};
pub use crate::schema::migrate;
pub use crate::store::{Fault, ScanStore, ScanSummary};

pub use crate::schema::CURRENT_SCHEMA;
