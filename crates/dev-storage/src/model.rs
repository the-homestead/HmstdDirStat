//! Report and aggregation types for developer storage analysis.

use std::{collections::BTreeMap, path::PathBuf};

use timberstat_storage_insights::{Category, InsightItem};

/// Historical growth requires the later scan-history feature.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum HistoryStatus {
    #[default]
    NotAvailable,
}

impl HistoryStatus {
    pub fn label(self) -> &'static str {
        "history not available"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IssueKind {
    /// A configured location was not represented by the scanned tree.
    UnknownLocation,
    /// The scanner could not completely read this path.
    Unreadable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportIssue {
    pub kind: IssueKind,
    pub path: PathBuf,
    pub message: String,
}

/// Per-category totals. `allocated_size` is `None` if any item has unknown
/// allocation; a partial allocated total is never presented as complete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategoryTotal {
    pub item_count: u64,
    pub logical_size: u64,
    pub allocated_size: Option<u64>,
}

impl Default for CategoryTotal {
    fn default() -> Self {
        Self {
            item_count: 0,
            logical_size: 0,
            allocated_size: Some(0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryDistribution {
    pub path: PathBuf,
    pub logical_size: u64,
    pub allocated_size: Option<u64>,
    pub complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevStorageReport {
    pub scan_root: PathBuf,
    pub items: Vec<InsightItem>,
    pub category_totals: BTreeMap<Category, CategoryTotal>,
    pub directory_distribution: Vec<DirectoryDistribution>,
    pub issues: Vec<ReportIssue>,
    pub historical_growth: HistoryStatus,
}

impl DevStorageReport {
    pub fn history_status(&self) -> &'static str {
        self.historical_growth.label()
    }

    pub fn largest_repositories(&self, limit: usize) -> Vec<&InsightItem> {
        let mut repositories: Vec<_> = self
            .items
            .iter()
            .filter(|item| item.category == Category::GitRepositories)
            .collect();
        sort_largest(&mut repositories);
        repositories.truncate(limit);
        repositories
    }

    pub fn largest_build_artifacts(&self, limit: usize) -> Vec<&InsightItem> {
        let mut artifacts: Vec<_> = self
            .items
            .iter()
            .filter(|item| is_build_artifact(item.category))
            .collect();
        sort_largest(&mut artifacts);
        artifacts.truncate(limit);
        artifacts
    }

    /// Candidates are ordered by estimated reclaimable bytes. This is a review
    /// queue, not approval to remove anything. Repository items are informational
    /// and therefore absent. Nested category totals must not be summed together.
    pub fn cleanup_candidates(&self) -> Vec<&InsightItem> {
        let mut candidates: Vec<_> = self
            .items
            .iter()
            .filter(|item| item.reclaimable_size > 0)
            .collect();
        candidates.sort_by(|left, right| {
            right
                .reclaimable_size
                .cmp(&left.reclaimable_size)
                .then_with(|| left.path.cmp(&right.path))
        });
        candidates
    }
}

fn sort_largest(items: &mut [&InsightItem]) {
    items.sort_by(|left, right| {
        right
            .logical_size
            .cmp(&left.logical_size)
            .then_with(|| left.path.cmp(&right.path))
    });
}

fn is_build_artifact(category: Category) -> bool {
    matches!(
        category,
        Category::CargoTargetDirectories
            | Category::RustBuildArtifacts
            | Category::CMakeBuildDirectories
            | Category::VisualStudioBuildArtifacts
    )
}
