//! Result types. A report lists groups and failures; it never says which copy
//! to keep.

use std::cmp::Reverse;
use std::path::PathBuf;

use hmstd_fileops::Failure;
use hmstd_filesystem::NodeId;

use crate::hash::{Digest, digest_hex};

/// One file in a duplicate group. `id` points back into the tree so the panel
/// can select it, reveal it, or hand it to the delete preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateMember {
    pub id: NodeId,
    pub path: PathBuf,
}

/// Byte-verified copies of one file. Every member has the same logical size
/// and identical bytes, and no two members are hard links of each other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateGroup {
    pub size: u64,
    pub digest: Digest,
    /// Sorted by path.
    pub members: Vec<DuplicateMember>,
}

impl DuplicateGroup {
    /// `(count - 1) * size`: the logical bytes that would be freed if every
    /// copy but one were removed. Logical, not on-disk, so it is kept apart
    /// from physical accounting.
    pub fn reclaimable(&self) -> u64 {
        let extra = (self.members.len() as u64).saturating_sub(1);
        self.size.saturating_mul(extra)
    }

    pub fn digest_hex(&self) -> String {
        digest_hex(&self.digest)
    }
}

/// How the run ended. A cancelled or failed run carries no groups.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RunStatus {
    #[default]
    Complete,
    Cancelled,
    Failed(String),
}

/// Full output of one run.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DuplicateReport {
    pub status: RunStatus,
    /// Sorted by reclaimable bytes, largest first.
    pub groups: Vec<DuplicateGroup>,
    /// Every file that could not be read or compared. Never silently dropped.
    pub failures: Vec<Failure>,
    /// Hard-linked paths left out because another path to the same object is
    /// already in the candidate set.
    pub hardlinks_skipped: u64,
    /// Files that share a size with at least one other file, after the
    /// minimum-size filter. Singleton sizes are not counted.
    pub files_considered: u64,
}

impl DuplicateReport {
    /// Sum of group reclaimable bytes. Logical bytes, not on-disk bytes.
    pub fn reclaimable_total(&self) -> u64 {
        self.groups
            .iter()
            .fold(0u64, |sum, group| sum.saturating_add(group.reclaimable()))
    }

    /// Number of files that appear in some group.
    pub fn duplicate_file_count(&self) -> usize {
        self.groups.iter().map(|group| group.members.len()).sum()
    }
}

/// Orders groups by reclaimable bytes, then size, then first path, and sorts
/// each group's members by path. Output is deterministic for the same input.
pub(crate) fn sort_groups(groups: &mut [DuplicateGroup]) {
    for group in groups.iter_mut() {
        group.members.sort_by(|a, b| a.path.cmp(&b.path));
    }
    groups.sort_by(|a, b| {
        (Reverse(a.reclaimable()), Reverse(a.size))
            .cmp(&(Reverse(b.reclaimable()), Reverse(b.size)))
            .then_with(|| a.members[0].path.cmp(&b.members[0].path))
    });
}
