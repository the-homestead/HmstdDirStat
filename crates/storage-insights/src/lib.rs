//! Review-only storage insights over an existing scanner [`timberstat_filesystem::FileTree`].
//!
//! Analysis never walks or writes to the filesystem. It classifies nodes into
//! categories with explicit reasons, logical/allocated sizes, conservative
//! reclaimable estimates, risk text, and one fixed `review only` action.
//! Duplicate-file detection remains owned by `timberstat-duplicates`.

mod engine;
mod locations;
mod model;
mod options;
mod patterns;
mod tree;

pub use engine::{analyse, analyse_with_locations};
pub use locations::{KnownLocation, LocationRole, LocationSet};
pub use model::{
    Category, InsightAction, InsightItem, InsightReport, ReclaimableBasis, Risk, RiskLevel,
    default_risk,
};
pub use options::{DAY_SECS, InsightsOptions, current_unix_secs};
