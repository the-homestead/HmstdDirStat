//! Convert a scanned tree into review-only storage recommendations.

use std::{collections::HashSet, path::Path};

use timberstat_filesystem::{EntryKind, FileIdentity, FileTree, Node, NodeId};

use crate::{
    Category, InsightAction, InsightItem, InsightReport, LocationRole, LocationSet,
    ReclaimableBasis,
    model::default_risk,
    options::{DAY_SECS, InsightsOptions},
    patterns,
    tree::TreeIndex,
};

/// Analyse with the current platform's known locations.
pub fn analyse(tree: &FileTree, scan_root: &Path, options: &InsightsOptions) -> InsightReport {
    analyse_with_locations(tree, scan_root, &LocationSet::platform_defaults(), options)
}

/// Analyse with explicit locations, allowing deterministic synthetic-tree use.
/// The tree must already be rolled up by the scanner. No disk traversal occurs.
pub fn analyse_with_locations(
    tree: &FileTree,
    scan_root: &Path,
    locations: &LocationSet,
    options: &InsightsOptions,
) -> InsightReport {
    let index = TreeIndex::build(tree, scan_root);
    let mut items = Vec::new();
    let mut claimed_nodes = HashSet::new();
    let mut claimed_identities = HashSet::new();
    let mut unknown_age_files = 0_u64;

    for id in tree.attached_ids() {
        let node = tree.node(id).expect("attached node must exist");
        if node.kind() != EntryKind::Directory || id == tree.root() || claimed_nodes.contains(&id) {
            continue;
        }
        let path = index.path(id).to_path_buf();

        if options.include_game_installs
            && let Some(location) = locations.game_installation(&path)
        {
            claimed_nodes.extend(index.subtree(id));
            let totals = node.totals();
            items.push(make_item(
                Category::GameInstalls,
                path,
                id,
                format!("direct installation directory under {}", location.label),
                SizeEstimate {
                    logical: totals.logical,
                    allocated: totals.physical_known.then_some(totals.physical),
                    reclaimable: 0,
                    basis: ReclaimableBasis::InformationalOnly,
                },
            ));
            continue;
        }

        if options.include_media_collections {
            let media = index.media(id);
            if media.files >= options.media_min_files && media.logical >= options.media_min_size {
                let totals = node.totals();
                let (reclaimable, basis) = if totals.physical_known {
                    (totals.physical, ReclaimableBasis::Allocated)
                } else {
                    (
                        index.unique_logical_under(id),
                        ReclaimableBasis::LogicalUpperBound,
                    )
                };
                claimed_nodes.extend(index.subtree(id));
                for file in index.files_under(id) {
                    if let Some(identity) = tree.node(file).and_then(|node| node.identity()) {
                        claimed_identities.insert(identity);
                    }
                }
                items.push(make_item(
                    Category::LargeMedia,
                    path,
                    id,
                    format!(
                        "{} media files total {} bytes; thresholds are {} files and {} bytes",
                        media.files, media.logical, options.media_min_files, options.media_min_size
                    ),
                    SizeEstimate {
                        logical: totals.logical,
                        allocated: totals.physical_known.then_some(totals.physical),
                        reclaimable,
                        basis,
                    },
                ));
                continue;
            }
        }

        if options.include_empty_directories
            && !node.is_unreadable()
            && tree.children(id).next().is_none()
        {
            let allocated = node.physical_size();
            items.push(make_item(
                Category::EmptyDirectories,
                path,
                id,
                "directory has no scanned entries and was readable".to_string(),
                SizeEstimate {
                    logical: 0,
                    allocated,
                    reclaimable: allocated.unwrap_or(0),
                    basis: if allocated.is_some() {
                        ReclaimableBasis::Allocated
                    } else {
                        ReclaimableBasis::LogicalUpperBound
                    },
                },
            ));
        }
    }

    for id in tree.attached_ids() {
        let node = tree.node(id).expect("attached node must exist");
        if node.kind() != EntryKind::File || claimed_nodes.contains(&id) {
            continue;
        }
        match classify_file(node, index.path(id), locations, options) {
            FileMatch::None => {}
            FileMatch::UnknownAge => unknown_age_files += 1,
            FileMatch::Match(category, reason) => {
                let (reclaimable, basis) = file_reclaim(node, &mut claimed_identities);
                items.push(make_item(
                    category,
                    index.path(id).to_path_buf(),
                    id,
                    reason,
                    SizeEstimate {
                        logical: node.logical_size(),
                        allocated: node.physical_size(),
                        reclaimable,
                        basis,
                    },
                ));
            }
        }
    }

    items.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.category.cmp(&right.category))
    });
    InsightReport {
        scan_root: scan_root.to_path_buf(),
        items,
        unknown_age_files,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FileMatch {
    None,
    UnknownAge,
    Match(Category, String),
}

fn classify_file(
    node: &Node,
    path: &Path,
    locations: &LocationSet,
    options: &InsightsOptions,
) -> FileMatch {
    let name = patterns::lower_name(node.name());
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    let flags = patterns::segment_flags(parent);

    if patterns::is_crash_dump_name(&name)
        || locations
            .match_role(path, LocationRole::CrashDumps)
            .is_some()
    {
        return FileMatch::Match(
            Category::CrashDumps,
            "crash-dump file name or registered crash-dump location".to_string(),
        );
    }
    if let Some(location) = locations.match_role(path, LocationRole::Temporary) {
        return aged_match(
            Category::TempFiles,
            format!("inside {}", location.label),
            node,
            options,
        );
    }
    if flags.browser && flags.cache {
        return FileMatch::Match(
            Category::BrowserCache,
            "path contains both browser and cache directory markers".to_string(),
        );
    }
    if flags.cache {
        return FileMatch::Match(
            Category::AppCache,
            "path contains an application cache directory marker".to_string(),
        );
    }
    if let Some(location) = locations.match_role(path, LocationRole::ApplicationCache) {
        return FileMatch::Match(Category::AppCache, format!("inside {}", location.label));
    }
    if patterns::is_log_name(&name)
        || flags.logs
        || locations.match_role(path, LocationRole::Logs).is_some()
    {
        return aged_match(
            Category::LogFiles,
            "log file name or registered log directory".to_string(),
            node,
            options,
        );
    }

    let extension = patterns::extension_of(node.name());
    if patterns::is_vm_extension(&extension) && node.logical_size() >= options.vm_image_min_size {
        return FileMatch::Match(
            Category::VmImages,
            format!(
                "VM-image extension and {} bytes meet the {}-byte threshold",
                node.logical_size(),
                options.vm_image_min_size
            ),
        );
    }
    if patterns::is_installer_name(&name) && node.logical_size() >= options.installer_min_size {
        return FileMatch::Match(
            Category::LargeInstallers,
            format!(
                "installer-style name and {} bytes meet the {}-byte threshold",
                node.logical_size(),
                options.installer_min_size
            ),
        );
    }
    if patterns::is_archive_name(&name) {
        return aged_match(
            Category::OldArchives,
            "archive file name extension".to_string(),
            node,
            options,
        );
    }
    if let Some(location) = locations.match_role(path, LocationRole::Downloads) {
        return aged_match(
            Category::OldDownloads,
            format!("inside {}", location.label),
            node,
            options,
        );
    }
    FileMatch::None
}

fn aged_match(
    category: Category,
    base_reason: String,
    node: &Node,
    options: &InsightsOptions,
) -> FileMatch {
    let Some(minimum) = options.age_threshold(category) else {
        return FileMatch::Match(category, base_reason);
    };
    let Some(modified) = node.metadata().modified_unix_secs else {
        return FileMatch::UnknownAge;
    };
    let age = options.now_unix_secs.saturating_sub(modified);
    if age < minimum {
        return FileMatch::None;
    }
    FileMatch::Match(
        category,
        format!(
            "{base_reason}; age {} days meets the configured {}-day threshold",
            age / DAY_SECS,
            minimum / DAY_SECS
        ),
    )
}

fn file_reclaim(
    node: &Node,
    claimed_identities: &mut HashSet<FileIdentity>,
) -> (u64, ReclaimableBasis) {
    if node.is_hardlink_duplicate() {
        return (0, ReclaimableBasis::SharedHardLink);
    }
    if node
        .identity()
        .is_some_and(|identity| !claimed_identities.insert(identity))
    {
        return (0, ReclaimableBasis::SharedHardLink);
    }
    node.physical_size().map_or(
        (node.logical_size(), ReclaimableBasis::LogicalUpperBound),
        |allocated| (allocated, ReclaimableBasis::Allocated),
    )
}

struct SizeEstimate {
    logical: u64,
    allocated: Option<u64>,
    reclaimable: u64,
    basis: ReclaimableBasis,
}

fn make_item(
    category: Category,
    path: std::path::PathBuf,
    node_id: NodeId,
    reason: String,
    sizes: SizeEstimate,
) -> InsightItem {
    InsightItem {
        category,
        path,
        node_id,
        reason,
        logical_size: sizes.logical,
        allocated_size: sizes.allocated,
        reclaimable_size: sizes.reclaimable,
        reclaimable_basis: sizes.basis,
        risk: default_risk(category),
        action: InsightAction::ReviewOnly,
    }
}
