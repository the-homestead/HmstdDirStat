//! Configurable thresholds for storage analysis.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::Category;

pub const DAY_SECS: i64 = 86_400;

/// Age thresholds are seconds; size thresholds are bytes. `None` for an age
/// threshold disables the age requirement, not the category.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InsightsOptions {
    pub temp_min_age: Option<i64>,
    pub downloads_min_age: Option<i64>,
    pub archives_min_age: Option<i64>,
    pub logs_min_age: Option<i64>,
    pub installer_min_size: u64,
    pub vm_image_min_size: u64,
    pub media_min_size: u64,
    pub media_min_files: u64,
    pub include_empty_directories: bool,
    pub include_media_collections: bool,
    /// Game installations are informational and always have zero reclaimable bytes.
    pub include_game_installs: bool,
    /// Fixed clock value used for every age comparison in one report.
    pub now_unix_secs: i64,
}

impl Default for InsightsOptions {
    fn default() -> Self {
        Self {
            temp_min_age: None,
            downloads_min_age: Some(30 * DAY_SECS),
            archives_min_age: Some(90 * DAY_SECS),
            logs_min_age: None,
            installer_min_size: 100 * 1024 * 1024,
            vm_image_min_size: 1024 * 1024 * 1024,
            media_min_size: 1024 * 1024 * 1024,
            media_min_files: 10,
            include_empty_directories: true,
            include_media_collections: true,
            include_game_installs: true,
            now_unix_secs: current_unix_secs(),
        }
    }
}

impl InsightsOptions {
    pub fn age_threshold(&self, category: Category) -> Option<i64> {
        match category {
            Category::TempFiles => self.temp_min_age,
            Category::OldDownloads => self.downloads_min_age,
            Category::OldArchives => self.archives_min_age,
            Category::LogFiles => self.logs_min_age,
            _ => None,
        }
    }
}

/// Current wall clock, clamped to the representable `i64` seconds range.
pub fn current_unix_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |after| {
            i64::try_from(after.as_secs()).unwrap_or(i64::MAX)
        })
}
