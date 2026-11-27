//! Hardware, OS and volume record written with each measured session.
//!
//! Hostname, user name, serial numbers and volume labels are deliberately left out.

use std::path::Path;

use serde::Serialize;
use sysinfo::{Disks, System};

use crate::run::SAMPLE_INTERVAL;

#[derive(Debug, Clone, Serialize)]
pub struct VolumeInfo {
    pub mount_point: String,
    pub file_system: String,
    pub kind: String,
    pub total_bytes: u64,
    pub removable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Environment {
    pub generated_unix: u64,
    pub perf_version: &'static str,
    pub build_profile: &'static str,
    pub target_os: &'static str,
    pub target_arch: &'static str,
    pub cpu_brand: Option<String>,
    pub cpu_physical_cores: Option<usize>,
    pub cpu_logical: usize,
    pub ram_total_bytes: u64,
    pub os_name: Option<String>,
    pub os_long_version: Option<String>,
    pub kernel_version: Option<String>,
    pub temp_dir: String,
    /// The volume that holds the temp directory, where datasets live.
    pub temp_volume: Option<VolumeInfo>,
    pub memory_sample_interval_ms: f64,
}

/// Collects the environment of the machine this process runs on.
pub fn collect(temp_dir: &Path) -> Environment {
    let system = System::new_all();
    let disks = Disks::new_with_refreshed_list();
    let temp_volume = disks
        .list()
        .iter()
        .filter(|disk| temp_dir.starts_with(disk.mount_point()))
        .max_by_key(|disk| disk.mount_point().as_os_str().len())
        .map(|disk| VolumeInfo {
            mount_point: disk.mount_point().display().to_string(),
            file_system: disk.file_system().to_string_lossy().into_owned(),
            kind: format!("{:?}", disk.kind()),
            total_bytes: disk.total_space(),
            removable: disk.is_removable(),
        });
    Environment {
        generated_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
        perf_version: env!("CARGO_PKG_VERSION"),
        build_profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        target_os: std::env::consts::OS,
        target_arch: std::env::consts::ARCH,
        cpu_brand: system.cpus().first().map(|cpu| cpu.brand().to_owned()),
        cpu_physical_cores: system.physical_core_count(),
        cpu_logical: system.cpus().len(),
        ram_total_bytes: system.total_memory(),
        os_name: System::name(),
        os_long_version: System::long_os_version(),
        kernel_version: System::kernel_version(),
        temp_dir: temp_dir.display().to_string(),
        temp_volume,
        memory_sample_interval_ms: SAMPLE_INTERVAL.as_secs_f64() * 1_000.0,
    }
}
