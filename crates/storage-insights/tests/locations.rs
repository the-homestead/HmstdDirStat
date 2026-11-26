use std::path::Path;

use timberstat_storage_insights::{LocationRole, LocationSet};

#[test]
fn location_matching_uses_whole_components_and_longest_role_prefix() {
    let locations = LocationSet::new()
        .add(LocationRole::Temporary, "/scan/Temp", "outer temp")
        .add(
            LocationRole::ApplicationCache,
            "/scan/Temp/Cache",
            "nested cache",
        );

    assert_eq!(
        locations
            .match_role(Path::new("/scan/Temp/work.bin"), LocationRole::Temporary)
            .map(|location| location.label.as_str()),
        Some("outer temp")
    );
    assert_eq!(
        locations
            .match_role(
                Path::new("/scan/Temp/Cache/work.bin"),
                LocationRole::ApplicationCache,
            )
            .map(|location| location.label.as_str()),
        Some("nested cache")
    );
    assert!(
        locations
            .match_role(Path::new("/scan/Temp2/work.bin"), LocationRole::Temporary)
            .is_none()
    );
}

#[cfg(windows)]
#[test]
fn windows_location_matching_is_case_insensitive() {
    let locations = LocationSet::new().add(LocationRole::Temporary, r"C:\Scan\Temp", "temp");
    assert!(
        locations
            .match_role(Path::new(r"c:\scan\temp\work.bin"), LocationRole::Temporary)
            .is_some()
    );
}

#[cfg(target_os = "linux")]
#[test]
fn linux_defaults_include_xdg_temp_log_and_crash_locations() {
    let locations = LocationSet::linux_defaults();
    assert!(
        locations
            .match_role(Path::new("/tmp/work.bin"), LocationRole::Temporary)
            .is_some()
    );
    assert!(
        locations
            .match_role(Path::new("/var/log/service.log"), LocationRole::Logs)
            .is_some()
    );
    let cache_entry = directories::BaseDirs::new()
        .expect("Linux base dirs")
        .cache_dir()
        .join("entry.bin");
    assert!(
        locations
            .match_role(&cache_entry, LocationRole::ApplicationCache)
            .is_some()
    );
}
