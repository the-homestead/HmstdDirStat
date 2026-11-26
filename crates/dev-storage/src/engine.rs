//! Convert a scanned tree into a review-only developer storage report.

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    path::Path,
};

use timberstat_filesystem::{EntryKind, FileTree, NodeId};
use timberstat_storage_insights::{
    Category, InsightAction, InsightItem, ReclaimableBasis, default_risk,
};

use crate::{
    DevLocationSet, DevStorageReport, HistoryStatus, IssueKind, ReportIssue,
    model::{CategoryTotal, DirectoryDistribution},
    patterns,
    tree::TreeIndex,
};

/// Analyse with the current platform's known cache and Docker/WSL locations.
pub fn analyse(tree: &FileTree, scan_root: &Path) -> DevStorageReport {
    analyse_with_locations(tree, scan_root, &DevLocationSet::platform_defaults())
}

/// Analyse with explicit locations for deterministic synthetic-tree use.
/// The tree must already be rolled up by the scanner. No disk traversal occurs.
pub fn analyse_with_locations(
    tree: &FileTree,
    scan_root: &Path,
    locations: &DevLocationSet,
) -> DevStorageReport {
    let index = TreeIndex::build(tree, scan_root);
    let mut items = Vec::new();
    let mut issues = Vec::new();
    let mut claimed_nodes = HashSet::new();
    let mut covered_locations = BTreeSet::new();

    for id in tree.attached_ids() {
        let node = index.node(id);
        if node.is_unreadable() {
            issues.push(ReportIssue {
                kind: IssueKind::Unreadable,
                path: index.path(id).to_path_buf(),
                message: "the scanner could not completely read this location; its size may be incomplete".to_string(),
            });
        }
        if claimed_nodes.contains(&id) {
            continue;
        }
        match node.kind() {
            EntryKind::Directory => classify_directory(
                &index,
                id,
                locations,
                &mut items,
                &mut claimed_nodes,
                &mut covered_locations,
            ),
            EntryKind::File => {
                if let Some(item) = classify_file(&index, id, locations, &mut covered_locations) {
                    items.push(item);
                }
            }
            EntryKind::Symlink | EntryKind::Other => {}
        }
    }

    for (position, location) in locations.locations().iter().enumerate() {
        if !covered_locations.contains(&position) {
            issues.push(ReportIssue {
                kind: IssueKind::UnknownLocation,
                path: location.display_path.clone(),
                message: format!(
                    "{} was not represented in the scanned tree; existence and size are unknown",
                    location.label
                ),
            });
        }
    }

    items.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.category.cmp(&right.category))
    });
    issues.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.kind.cmp(&right.kind))
    });
    let category_totals = category_totals(&items);
    let directory_distribution = directory_distribution(&index, tree.root());
    DevStorageReport {
        scan_root: scan_root.to_path_buf(),
        items,
        category_totals,
        directory_distribution,
        issues,
        historical_growth: HistoryStatus::NotAvailable,
    }
}

fn classify_directory(
    index: &TreeIndex<'_>,
    id: NodeId,
    locations: &DevLocationSet,
    items: &mut Vec<InsightItem>,
    claimed_nodes: &mut HashSet<NodeId>,
    covered_locations: &mut BTreeSet<usize>,
) {
    let path = index.path(id);
    if index.has_marker(id, ".git") {
        items.push(directory_item(
            index,
            id,
            Category::GitRepositories,
            "directory contains a .git directory or worktree file; repository size is informational".to_string(),
            true,
        ));
    }

    if let Some((position, location)) = locations.directory_match(path) {
        covered_locations.insert(position);
        items.push(directory_item(
            index,
            id,
            location.role.category(),
            format!("inside or at {}", location.label),
            false,
        ));
        claimed_nodes.extend(index.subtree(id));
        return;
    }

    let Some((category, reason)) = directory_match(index, id) else {
        return;
    };
    items.push(directory_item(index, id, category, reason, false));
    // A detected build/cache root is one atomic report item. Nested output is
    // sized by that root rather than emitted again as duplicate candidates.
    claimed_nodes.extend(index.subtree(id));
}

fn directory_match(index: &TreeIndex<'_>, id: NodeId) -> Option<(Category, String)> {
    let node = index.node(id);
    let name = patterns::lower(node.name());
    if name == "node_modules" {
        return Some((
            Category::NodeModules,
            "directory is named node_modules".to_string(),
        ));
    }
    if name == "target"
        && index.has_file_marker(id, "CACHEDIR.TAG")
        && node
            .parent()
            .is_some_and(|parent| index.has_file_marker(parent, "Cargo.toml"))
    {
        return Some((
            Category::CargoTargetDirectories,
            "target contains CACHEDIR.TAG and its sibling Cargo.toml identifies the Cargo workspace".to_string(),
        ));
    }
    if index.has_file_marker(id, "pyvenv.cfg") {
        return Some((
            Category::PythonVirtualEnvironments,
            "directory contains pyvenv.cfg".to_string(),
        ));
    }
    if index.has_file_marker(id, "CMakeCache.txt") {
        return Some((
            Category::CMakeBuildDirectories,
            "directory contains CMakeCache.txt".to_string(),
        ));
    }
    if patterns::is_yarn_project_cache(index.path(id)) {
        return Some((
            Category::YarnCaches,
            "directory is the cache inside a .yarn directory".to_string(),
        ));
    }
    if patterns::is_visual_studio_directory(node.name()) && index.has_visual_studio_context(id) {
        return Some((
            Category::VisualStudioBuildArtifacts,
            "Visual Studio output directory with a solution or project marker in its parent or grandparent".to_string(),
        ));
    }
    if patterns::is_rust_artifact_directory(node.name()) && index.contains_rust_artifact(id) {
        return Some((
            Category::RustBuildArtifacts,
            "Rust build-output directory contains .rlib or .rmeta artifacts and is outside a recognized Cargo target".to_string(),
        ));
    }
    None
}

fn classify_file(
    index: &TreeIndex<'_>,
    id: NodeId,
    locations: &DevLocationSet,
    covered_locations: &mut BTreeSet<usize>,
) -> Option<InsightItem> {
    let node = index.node(id);
    let path = index.path(id);
    if let Some((position, location)) = locations.file_match(path) {
        covered_locations.insert(position);
        return Some(file_item(
            index,
            id,
            location.role.category(),
            format!("exactly matches {}", location.label),
        ));
    }
    if patterns::is_docker_or_wsl_disk(path) {
        return Some(file_item(
            index,
            id,
            Category::DockerStorage,
            "VHDX path has Docker Desktop or WSL package markers; size is read from the scan only"
                .to_string(),
        ));
    }
    if patterns::is_vm_image(node.name()) {
        return Some(file_item(
            index,
            id,
            Category::VmImages,
            "file has a virtual-machine disk extension".to_string(),
        ));
    }
    if patterns::is_rust_artifact(node.name()) {
        return Some(file_item(
            index,
            id,
            Category::RustBuildArtifacts,
            "standalone .rlib or .rmeta Rust build artifact".to_string(),
        ));
    }
    None
}

fn directory_item(
    index: &TreeIndex<'_>,
    id: NodeId,
    category: Category,
    reason: String,
    informational: bool,
) -> InsightItem {
    let totals = index.node(id).totals();
    let allocated = totals.physical_known.then_some(totals.physical);
    make_item(
        id,
        index,
        category,
        reason,
        totals.logical,
        allocated,
        informational,
    )
}

fn file_item(index: &TreeIndex<'_>, id: NodeId, category: Category, reason: String) -> InsightItem {
    let node = index.node(id);
    make_item(
        id,
        index,
        category,
        reason,
        node.logical_size(),
        node.physical_size(),
        false,
    )
}

fn make_item(
    id: NodeId,
    index: &TreeIndex<'_>,
    category: Category,
    reason: String,
    logical: u64,
    allocated: Option<u64>,
    informational: bool,
) -> InsightItem {
    let (reclaimable, basis) = if informational {
        (0, ReclaimableBasis::InformationalOnly)
    } else {
        allocated.map_or(
            (logical, ReclaimableBasis::LogicalUpperBound),
            |allocated| (allocated, ReclaimableBasis::Allocated),
        )
    };
    InsightItem {
        category,
        path: index.path(id).to_path_buf(),
        node_id: id,
        reason,
        logical_size: logical,
        allocated_size: allocated,
        reclaimable_size: reclaimable,
        reclaimable_basis: basis,
        risk: default_risk(category),
        action: InsightAction::ReviewOnly,
    }
}

fn category_totals(items: &[InsightItem]) -> BTreeMap<Category, CategoryTotal> {
    let mut totals = BTreeMap::new();
    for item in items {
        let total: &mut CategoryTotal = totals.entry(item.category).or_default();
        total.item_count = total.item_count.saturating_add(1);
        total.logical_size = total.logical_size.saturating_add(item.logical_size);
        total.allocated_size = match (total.allocated_size, item.allocated_size) {
            (Some(current), Some(additional)) => Some(current.saturating_add(additional)),
            _ => None,
        };
    }
    totals
}

fn directory_distribution(index: &TreeIndex<'_>, root: NodeId) -> Vec<DirectoryDistribution> {
    let mut distribution: Vec<_> = index
        .children(root)
        .filter(|id| index.node(*id).kind() == EntryKind::Directory)
        .map(|id| {
            let totals = index.node(id).totals();
            DirectoryDistribution {
                path: index.path(id).to_path_buf(),
                logical_size: totals.logical,
                allocated_size: totals.physical_known.then_some(totals.physical),
                complete: totals.complete,
            }
        })
        .collect();
    distribution.sort_by(|left, right| {
        right
            .logical_size
            .cmp(&left.logical_size)
            .then_with(|| left.path.cmp(&right.path))
    });
    distribution
}
