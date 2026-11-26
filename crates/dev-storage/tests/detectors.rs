mod common;

use common::{Fixture, analyse, complete_report};
use timberstat_dev_storage::{
    Category, DevLocationSet, DevStorageReport, InsightAction, analyse_with_locations,
};
use timberstat_filesystem::NodeId;

fn item(report: &DevStorageReport, category: Category) -> &timberstat_dev_storage::InsightItem {
    let items: Vec<_> = report
        .items
        .iter()
        .filter(|item| item.category == category)
        .collect();
    assert_eq!(items.len(), 1, "category {category:?}: {items:#?}");
    items[0]
}

#[test]
fn detects_git_repository_and_keeps_it_informational() {
    let (_temp, report) = complete_report();
    let item = item(&report, Category::GitRepositories);
    assert!(item.reason.contains(".git"));
    assert_eq!(item.reclaimable_size, 0);
}

#[test]
fn detects_cargo_target_from_cachedir_and_sibling_manifest() {
    let (_temp, report) = complete_report();
    let item = item(&report, Category::CargoTargetDirectories);
    assert!(item.reason.contains("CACHEDIR.TAG"));
    assert!(item.reason.contains("Cargo.toml"));
    assert_eq!(item.logical_size, 303);
}

#[test]
fn detects_rust_build_artifacts_outside_cargo_target() {
    let (_temp, report) = complete_report();
    let item = item(&report, Category::RustBuildArtifacts);
    assert!(item.path.ends_with("rust-output/debug"));
    assert_eq!(item.logical_size, 80);
}

#[test]
fn detects_node_modules() {
    let (_temp, report) = complete_report();
    assert_eq!(item(&report, Category::NodeModules).logical_size, 90);
}

#[test]
fn detects_npm_cache() {
    let (_temp, report) = complete_report();
    assert_eq!(item(&report, Category::NpmCaches).logical_size, 20);
}

#[test]
fn detects_pnpm_cache_and_store() {
    let (_temp, report) = complete_report();
    assert_eq!(item(&report, Category::PnpmCaches).logical_size, 21);
}

#[test]
fn detects_yarn_cache() {
    let (_temp, report) = complete_report();
    assert_eq!(item(&report, Category::YarnCaches).logical_size, 22);
}

#[test]
fn detects_python_virtual_environment_from_pyvenv_cfg() {
    let (_temp, report) = complete_report();
    let item = item(&report, Category::PythonVirtualEnvironments);
    assert!(item.reason.contains("pyvenv.cfg"));
    assert_eq!(item.logical_size, 54);
}

#[test]
fn detects_pip_cache() {
    let (_temp, report) = complete_report();
    assert_eq!(item(&report, Category::PipCaches).logical_size, 23);
}

#[test]
fn detects_gradle_caches() {
    let (_temp, report) = complete_report();
    assert_eq!(item(&report, Category::GradleCaches).logical_size, 24);
}

#[test]
fn detects_maven_m2_repository() {
    let (_temp, report) = complete_report();
    let item = item(&report, Category::MavenRepositories);
    assert!(item.path.ends_with("maven/.m2"));
    assert_eq!(item.logical_size, 32);
}

#[test]
fn detects_cmake_build_directory_from_cache_marker() {
    let (_temp, report) = complete_report();
    let item = item(&report, Category::CMakeBuildDirectories);
    assert!(item.reason.contains("CMakeCache.txt"));
    assert_eq!(item.logical_size, 65);
}

#[test]
fn detects_visual_studio_build_artifacts() {
    let (_temp, report) = complete_report();
    let item = item(&report, Category::VisualStudioBuildArtifacts);
    assert!(item.path.ends_with("vs-app/x64"));
    assert_eq!(item.logical_size, 70);
}

#[test]
fn detects_docker_desktop_vhdx_without_treating_it_as_a_generic_vm() {
    let (_temp, report) = complete_report();
    let docker = item(&report, Category::DockerStorage);
    assert_eq!(docker.logical_size, 200);
    assert!(docker.reason.contains("Docker"));
    assert!(
        report
            .items
            .iter()
            .all(|item| item.path != docker.path || item.category == Category::DockerStorage)
    );
}

#[test]
fn detects_virtual_machine_images() {
    let (_temp, report) = complete_report();
    assert_eq!(item(&report, Category::VmImages).logical_size, 220);
}

#[test]
fn git_worktree_file_is_a_repository_marker() {
    let mut fixture = Fixture::new();
    let worktree = fixture.dir(NodeId::ROOT, "worktree");
    fixture.file(worktree, ".git", 12);
    fixture.file(worktree, "source.rs", 30);
    let (_temp, report) = analyse(fixture);
    assert_eq!(item(&report, Category::GitRepositories).logical_size, 42);
}

#[test]
fn cargo_target_requires_both_marker_and_sibling_manifest() {
    let mut fixture = Fixture::new();
    let target = fixture.dir(NodeId::ROOT, "target");
    fixture.file(target, "CACHEDIR.TAG", 3);
    fixture.file(target, "output.bin", 50);
    let (_temp, report) = analyse(fixture);
    assert!(
        report
            .items
            .iter()
            .all(|item| item.category != Category::CargoTargetDirectories)
    );
}

#[test]
fn project_yarn_cache_needs_no_registered_location() {
    let mut fixture = Fixture::new();
    let cache = fixture.dirs(NodeId::ROOT, &["web", ".yarn", "cache"]);
    fixture.file(cache, "package.zip", 44);
    let (temp, tree) = fixture.finish();
    let report = analyse_with_locations(&tree, temp.path(), &DevLocationSet::new());
    assert_eq!(item(&report, Category::YarnCaches).logical_size, 44);
}

#[test]
fn wsl_package_vhdx_is_docker_storage_not_a_generic_vm_image() {
    let mut fixture = Fixture::new();
    let state = fixture.dirs(NodeId::ROOT, &["Packages", "Distro", "LocalState"]);
    fixture.file(state, "ext4.vhdx", 500);
    let (temp, tree) = fixture.finish();
    let report = analyse_with_locations(&tree, temp.path(), &DevLocationSet::new());
    assert_eq!(item(&report, Category::DockerStorage).logical_size, 500);
    assert!(
        report
            .items
            .iter()
            .all(|item| item.category != Category::VmImages)
    );
}

#[test]
fn every_detected_item_is_review_only_even_in_detector_tests() {
    let (_temp, report) = complete_report();
    assert!(!report.items.is_empty());
    assert!(
        report
            .items
            .iter()
            .all(|item| item.action == InsightAction::ReviewOnly)
    );
}
