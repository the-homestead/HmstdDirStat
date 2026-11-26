//! Duplicate detection for TimberStat: a staged pipeline that never deletes.
//!
//! Stages, each more expensive than the last:
//! 1. group files by logical size; drop singletons;
//! 2. resolve file identity and keep one path per hard-linked object;
//! 3. digest the first and last 64 KiB and drop files that differ there;
//! 4. hash the whole file with BLAKE3;
//! 5. byte-compare each hash class before a group is reported.
//!
//! GPUI-independent. The pipeline runs on a bounded rayon pool it builds for
//! itself, never the global pool. Output is a report: nothing here removes or
//! modifies a file, and no member is marked as the keeper.

mod hash;
mod identity;
mod items;
mod pipeline;
mod progress;
mod report;
mod sample;

#[cfg(test)]
mod verify_tests;

pub use hash::{Digest, digest_hex};
pub use identity::file_key;
pub use items::file_items;
pub use pipeline::{Options, find_duplicates};
pub use progress::{Progress, Stage};
pub use report::{DuplicateGroup, DuplicateMember, DuplicateReport, RunStatus};
pub use sample::SAMPLE_BYTES;
