//! Read-only developer storage analysis over an existing
//! [`timberstat_filesystem::FileTree`].
//!
//! The analyser performs no filesystem traversal or mutation. It detects source
//! repositories, build output, package-manager caches, virtual environments,
//! Docker/WSL disks, and VM images from the scanned tree. Every recommendation
//! reuses the storage-insights item model and its fixed `review only` action.

mod engine;
mod locations;
mod model;
mod patterns;
mod tree;

pub use engine::{analyse, analyse_with_locations};
pub use locations::{DevLocationSet, KnownLocation, LocationKind, LocationRole};
pub use model::{
    CategoryTotal, DevStorageReport, DirectoryDistribution, HistoryStatus, IssueKind, ReportIssue,
};
pub use timberstat_storage_insights::{
    Category, InsightAction, InsightItem, ReclaimableBasis, Risk, RiskLevel,
};
