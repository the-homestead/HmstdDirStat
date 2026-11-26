//! Read-only comparison of persisted TimberStat scans.
//!
//! Load [`timberstat_scan_history::Scan`] values with `ScanStore`, then use
//! [`compare`] for a path-based diff or [`growth_trend`] for an oldest-to-newest
//! series. This crate performs no filesystem writes and contains no GUI types.

mod compare;
mod error;
mod index;
mod model;
mod trend;

pub use compare::compare;
pub use error::ComparisonError;
pub use model::{
    ChangeCategory, DirectoryDelta, EntryChange, EntrySnapshot, GrowthTrend, ModificationReasons,
    ScanComparison, SizeDelta, SizeTotals, TrendInterval, TrendSnapshot,
};
pub use trend::growth_trend;
