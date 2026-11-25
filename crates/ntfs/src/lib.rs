//! NTFS Master File Table reader: boot sector, runlists, fixups, attributes,
//! and conversion into the shared `FileTree`. Safe Rust only. Raw volume access
//! needs administrator rights; the caller supplies any `Read + Seek` source, so
//! the parser is tested on in-memory images.

mod boot;
mod bytes;
mod catalog;
mod error;
mod reader;
mod record;
mod runlist;
mod summary;

pub use boot::{BootSector, parse_boot_sector};
pub use catalog::{BuildStats, Catalog, CatalogEntry};
pub use error::NtfsError;
pub use reader::{ROOT_RECORD, ReadStats, read_records};
pub use record::{DataInfo, NameEntry, Record, parse_record};
pub use runlist::Run;
pub use summary::{VolumeSummary, summarize_volume};
