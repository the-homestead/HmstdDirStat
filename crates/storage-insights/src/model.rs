//! Result types for review-only storage recommendations.

use std::{collections::BTreeMap, path::PathBuf};

use timberstat_filesystem::NodeId;

/// The kind of storage a review candidate belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    TempFiles,
    AppCache,
    BrowserCache,
    LogFiles,
    OldDownloads,
    LargeInstallers,
    OldArchives,
    CrashDumps,
    EmptyDirectories,
    VmImages,
    LargeMedia,
    GameInstalls,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Self::TempFiles => "Temporary files",
            Self::AppCache => "Application caches",
            Self::BrowserCache => "Browser caches",
            Self::LogFiles => "Log files",
            Self::OldDownloads => "Old downloads",
            Self::LargeInstallers => "Large installers",
            Self::OldArchives => "Old archives",
            Self::CrashDumps => "Crash dumps",
            Self::EmptyDirectories => "Empty directories",
            Self::VmImages => "Virtual machine images",
            Self::LargeMedia => "Large media collections",
            Self::GameInstalls => "Game installations",
        }
    }
}

/// How much care an item needs before a human changes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

/// Risk information shown beside a candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Risk {
    pub level: RiskLevel,
    pub text: String,
}

impl Risk {
    pub fn new(level: RiskLevel, text: impl Into<String>) -> Self {
        Self {
            level,
            text: text.into(),
        }
    }
}

/// The only action exposed by this crate. There is deliberately no delete,
/// prune, trash, move, or cleanup variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsightAction {
    ReviewOnly,
}

impl InsightAction {
    pub fn label(self) -> &'static str {
        "review only"
    }
}

/// How the reclaimable estimate was calculated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReclaimableBasis {
    /// Allocated bytes recorded by the scanner.
    Allocated,
    /// Logical bytes used as an estimate when allocation is unknown.
    LogicalUpperBound,
    /// Another item already carries this file identity's single charge.
    SharedHardLink,
    /// The item describes an installation; it is not a reclaim recommendation.
    InformationalOnly,
}

/// One review candidate. All sizes are in bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InsightItem {
    pub category: Category,
    pub path: PathBuf,
    pub node_id: NodeId,
    /// Why the path matched this category, including any threshold used.
    pub reason: String,
    pub logical_size: u64,
    /// `None` means the scanner could not determine allocated size.
    pub allocated_size: Option<u64>,
    pub reclaimable_size: u64,
    pub reclaimable_basis: ReclaimableBasis,
    pub risk: Risk,
    pub action: InsightAction,
}

/// The candidates produced by one read-only analysis pass.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InsightReport {
    pub scan_root: PathBuf,
    pub items: Vec<InsightItem>,
    /// Files matching an age-based rule whose modification time was unknown.
    pub unknown_age_files: u64,
}

impl InsightReport {
    pub fn item_count(&self) -> usize {
        self.items.len()
    }

    /// Sum of estimates after hard-link de-duplication during analysis.
    pub fn reclaimable_total(&self) -> u64 {
        self.items.iter().fold(0_u64, |total, item| {
            total.saturating_add(item.reclaimable_size)
        })
    }

    pub fn reclaimable_by_category(&self) -> BTreeMap<Category, u64> {
        let mut totals = BTreeMap::new();
        for item in &self.items {
            let total = totals.entry(item.category).or_insert(0_u64);
            *total = total.saturating_add(item.reclaimable_size);
        }
        totals
    }

    pub fn items_in(&self, category: Category) -> Vec<&InsightItem> {
        self.items
            .iter()
            .filter(|item| item.category == category)
            .collect()
    }
}

/// Conservative risk text; age and size never imply that removal is safe.
pub fn default_risk(category: Category) -> Risk {
    let (level, text) = match category {
        Category::TempFiles => (
            RiskLevel::Medium,
            "Temporary files can belong to a running application or contain unfinished work. Check the owner and whether the file is in use.",
        ),
        Category::AppCache => (
            RiskLevel::Medium,
            "An application cache can contain offline data, credentials, or state not stored elsewhere. Identify the owning application before changing it.",
        ),
        Category::BrowserCache => (
            RiskLevel::Medium,
            "A browser cache belongs to a browser profile. Changing it can affect offline data, sessions, and subsequent browser performance.",
        ),
        Category::LogFiles => (
            RiskLevel::Medium,
            "A log can be the only record of a failure and can still be open by a service. Check its diagnostic value and owner.",
        ),
        Category::OldDownloads => (
            RiskLevel::Medium,
            "A download can be the only copy of needed content. Age alone does not establish that it is unwanted.",
        ),
        Category::LargeInstallers => (
            RiskLevel::Medium,
            "An installer can be the only available way to reinstall or repair an application. Verify its purpose and another installation source.",
        ),
        Category::OldArchives => (
            RiskLevel::Medium,
            "An archive can be the only copy of its contents. Inspect the contents and verify another copy before changing it.",
        ),
        Category::CrashDumps => (
            RiskLevel::High,
            "A crash dump can contain sensitive data and can be the only evidence of a failure. Check whether diagnosis is complete.",
        ),
        Category::EmptyDirectories => (
            RiskLevel::Low,
            "An application can expect an empty directory to exist as a working or mount directory. Identify what created it.",
        ),
        Category::VmImages => (
            RiskLevel::High,
            "A virtual machine image can contain an entire system and all of its data. Verify the VM, snapshots, and backups before changing it.",
        ),
        Category::LargeMedia => (
            RiskLevel::High,
            "A media collection is user content. Size and file type do not show whether it is wanted or backed up.",
        ),
        Category::GameInstalls => (
            RiskLevel::High,
            "Informational only. A game installation is managed by its launcher; changing individual files can break the installation or saves.",
        ),
    };
    Risk::new(level, text)
}
