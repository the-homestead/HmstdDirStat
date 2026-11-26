use std::path::Path;

use timberstat_dev_storage::{DevLocationSet, LocationRole};

#[test]
fn known_directory_matching_uses_whole_components_and_longest_prefix() {
    let locations = DevLocationSet::new()
        .add_directory(LocationRole::NpmCache, "/scan/npm", "npm")
        .add_directory(LocationRole::PnpmCache, "/scan/npm/pnpm", "pnpm");
    assert_eq!(
        locations
            .match_directory(Path::new("/scan/npm/pnpm/store"))
            .map(|location| location.role),
        Some(LocationRole::PnpmCache)
    );
    assert!(
        locations
            .match_directory(Path::new("/scan/npm2/store"))
            .is_none()
    );
}

#[cfg(windows)]
#[test]
fn windows_defaults_are_docker_and_wsl_first() {
    let locations = DevLocationSet::windows_defaults();
    if std::env::var_os("LOCALAPPDATA").is_some() {
        assert!(locations.locations().iter().any(|location| {
            location.role == LocationRole::DockerStorage
                && location.display_path.ends_with("Docker/wsl/data/ext4.vhdx")
        }));
        assert!(locations.locations().iter().any(|location| {
            location.role == LocationRole::DockerStorage
                && location
                    .display_path
                    .ends_with("Docker/wsl/disk/docker_data.vhdx")
        }));
    }
}

#[cfg(target_os = "linux")]
#[test]
fn linux_defaults_include_package_caches_gradle_and_maven() {
    let locations = DevLocationSet::linux_defaults();
    assert!(
        locations
            .locations()
            .iter()
            .any(|location| location.role == LocationRole::PipCache)
    );
    assert!(
        locations
            .locations()
            .iter()
            .any(|location| location.role == LocationRole::GradleCache)
    );
    assert!(
        locations
            .locations()
            .iter()
            .any(|location| location.role == LocationRole::MavenRepository)
    );
}
