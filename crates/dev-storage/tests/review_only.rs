mod common;

use common::complete_report;
use timberstat_dev_storage::{Category, InsightAction, ReclaimableBasis};

const ALL_CATEGORIES: [Category; 15] = [
    Category::GitRepositories,
    Category::CargoTargetDirectories,
    Category::RustBuildArtifacts,
    Category::NodeModules,
    Category::NpmCaches,
    Category::PnpmCaches,
    Category::YarnCaches,
    Category::PythonVirtualEnvironments,
    Category::PipCaches,
    Category::GradleCaches,
    Category::MavenRepositories,
    Category::CMakeBuildDirectories,
    Category::VisualStudioBuildArtifacts,
    Category::DockerStorage,
    Category::VmImages,
];

#[test]
fn every_output_item_is_review_only_and_analysis_preserves_every_path() {
    let (temp, report) = complete_report();
    assert_eq!(report.category_totals.len(), ALL_CATEGORIES.len());
    for category in ALL_CATEGORIES {
        assert!(
            report.items.iter().any(|item| item.category == category),
            "missing {category:?}"
        );
    }
    assert_eq!(report.items.len(), ALL_CATEGORIES.len(), "{report:#?}");
    for item in &report.items {
        assert_eq!(item.action, InsightAction::ReviewOnly);
        assert_eq!(item.action.label(), "review only");
        assert!(
            item.path.exists(),
            "analysis changed or removed {}",
            item.path.display()
        );
        if item.category == Category::GitRepositories {
            assert_eq!(item.reclaimable_size, 0);
            assert_eq!(item.reclaimable_basis, ReclaimableBasis::InformationalOnly);
        } else {
            assert!(item.reclaimable_size > 0, "category {:?}", item.category);
        }
    }
    assert!(temp.path().exists());
    assert!(
        report
            .cleanup_candidates()
            .iter()
            .all(|item| item.action == InsightAction::ReviewOnly)
    );
}
