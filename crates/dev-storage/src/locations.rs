//! Known cache and virtual-disk locations matched against an existing scan.
//! Registering or matching a path performs no filesystem access.

use std::path::{Path, PathBuf};

#[cfg(windows)]
use std::{env, path::Component};

use timberstat_storage_insights::Category;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocationRole {
    NpmCache,
    PnpmCache,
    YarnCache,
    PipCache,
    GradleCache,
    MavenRepository,
    DockerStorage,
}

impl LocationRole {
    pub(crate) fn category(self) -> Category {
        match self {
            Self::NpmCache => Category::NpmCaches,
            Self::PnpmCache => Category::PnpmCaches,
            Self::YarnCache => Category::YarnCaches,
            Self::PipCache => Category::PipCaches,
            Self::GradleCache => Category::GradleCaches,
            Self::MavenRepository => Category::MavenRepositories,
            Self::DockerStorage => Category::DockerStorage,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocationKind {
    Directory,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownLocation {
    pub role: LocationRole,
    pub kind: LocationKind,
    pub match_path: PathBuf,
    pub display_path: PathBuf,
    pub label: String,
}

/// Configurable known locations. Tests and callers can supply synthetic roots.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DevLocationSet {
    locations: Vec<KnownLocation>,
}

impl DevLocationSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_directory(
        self,
        role: LocationRole,
        path: impl Into<PathBuf>,
        label: impl Into<String>,
    ) -> Self {
        self.add(role, LocationKind::Directory, path, label)
    }

    pub fn add_file(
        self,
        role: LocationRole,
        path: impl Into<PathBuf>,
        label: impl Into<String>,
    ) -> Self {
        self.add(role, LocationKind::File, path, label)
    }

    fn add(
        mut self,
        role: LocationRole,
        kind: LocationKind,
        path: impl Into<PathBuf>,
        label: impl Into<String>,
    ) -> Self {
        let display_path = path.into();
        let match_path = normalise_for_match(&display_path);
        if !self.locations.iter().any(|existing| {
            existing.role == role && existing.kind == kind && existing.match_path == match_path
        }) {
            self.locations.push(KnownLocation {
                role,
                kind,
                match_path,
                display_path,
                label: label.into(),
            });
        }
        self
    }

    pub fn locations(&self) -> &[KnownLocation] {
        &self.locations
    }

    /// Longest known directory prefix. This also classifies a scan root inside
    /// a known cache, without probing the cache outside the scanned tree.
    pub(crate) fn directory_match(&self, path: &Path) -> Option<(usize, &KnownLocation)> {
        let needle = normalise_for_match(path);
        self.locations
            .iter()
            .enumerate()
            .filter(|(_, location)| location.kind == LocationKind::Directory)
            .filter(|(_, location)| starts_with_components(&needle, &location.match_path))
            .max_by_key(|(_, location)| location.match_path.components().count())
    }

    pub(crate) fn file_match(&self, path: &Path) -> Option<(usize, &KnownLocation)> {
        let needle = normalise_for_match(path);
        self.locations.iter().enumerate().find(|(_, location)| {
            location.kind == LocationKind::File && needle == location.match_path
        })
    }

    pub fn match_directory(&self, path: &Path) -> Option<&KnownLocation> {
        self.directory_match(path).map(|(_, location)| location)
    }

    pub fn match_file(&self, path: &Path) -> Option<&KnownLocation> {
        self.file_match(path).map(|(_, location)| location)
    }

    #[cfg(windows)]
    pub fn windows_defaults() -> Self {
        let user_profile = env::var_os("USERPROFILE").map(PathBuf::from);
        let local = env::var_os("LOCALAPPDATA").map(PathBuf::from);
        let roaming = env::var_os("APPDATA").map(PathBuf::from);
        let program_data = env::var_os("ProgramData").map(PathBuf::from);
        let mut locations = Self::new();
        if let Some(user) = user_profile {
            locations = locations
                .add_directory(
                    LocationRole::NpmCache,
                    user.join(".npm"),
                    "the user npm cache",
                )
                .add_directory(
                    LocationRole::PnpmCache,
                    user.join(".pnpm-store"),
                    "the user pnpm store",
                )
                .add_directory(
                    LocationRole::GradleCache,
                    user.join(".gradle/caches"),
                    "the user Gradle caches",
                )
                .add_directory(
                    LocationRole::MavenRepository,
                    user.join(".m2"),
                    "the user Maven .m2 directory",
                );
        }
        if let Some(local) = local {
            locations = locations
                .add_directory(
                    LocationRole::PnpmCache,
                    local.join("pnpm-cache"),
                    "the LOCALAPPDATA pnpm cache",
                )
                .add_directory(
                    LocationRole::PnpmCache,
                    local.join("pnpm/store"),
                    "the LOCALAPPDATA pnpm store",
                )
                .add_directory(
                    LocationRole::YarnCache,
                    local.join("Yarn/Cache"),
                    "the LOCALAPPDATA Yarn cache",
                )
                .add_directory(
                    LocationRole::PipCache,
                    local.join("pip/Cache"),
                    "the LOCALAPPDATA pip cache",
                )
                .add_file(
                    LocationRole::DockerStorage,
                    local.join("Docker/wsl/data/ext4.vhdx"),
                    "the Docker Desktop data VHDX",
                )
                .add_file(
                    LocationRole::DockerStorage,
                    local.join("Docker/wsl/disk/docker_data.vhdx"),
                    "the Docker Desktop docker_data VHDX",
                );
        }
        if let Some(roaming) = roaming {
            locations = locations.add_directory(
                LocationRole::NpmCache,
                roaming.join("npm-cache"),
                "the APPDATA npm cache",
            );
        }
        if let Some(program_data) = program_data {
            locations = locations.add_file(
                LocationRole::DockerStorage,
                program_data.join("DockerDesktop/vm-data/DockerDesktop.vhdx"),
                "the ProgramData Docker Desktop VHDX",
            );
        }
        locations
    }

    #[cfg(target_os = "linux")]
    pub fn linux_defaults() -> Self {
        let home = env::var_os("HOME").map(PathBuf::from);
        let cache = env::var_os("XDG_CACHE_HOME").map(PathBuf::from);
        let data = env::var_os("XDG_DATA_HOME").map(PathBuf::from);
        let Some(home) = home else {
            return Self::new();
        };
        let cache = cache.unwrap_or_else(|| home.join(".cache"));
        let data = data.unwrap_or_else(|| home.join(".local/share"));
        Self::new()
            .add_directory(
                LocationRole::NpmCache,
                home.join(".npm"),
                "the user npm cache",
            )
            .add_directory(
                LocationRole::PnpmCache,
                home.join(".pnpm-store"),
                "the user pnpm store",
            )
            .add_directory(
                LocationRole::PnpmCache,
                cache.join("pnpm"),
                "the XDG pnpm cache",
            )
            .add_directory(
                LocationRole::PnpmCache,
                data.join("pnpm/store"),
                "the XDG pnpm store",
            )
            .add_directory(
                LocationRole::YarnCache,
                cache.join("yarn"),
                "the XDG Yarn cache",
            )
            .add_directory(
                LocationRole::YarnCache,
                home.join(".yarn/cache"),
                "the user Yarn cache",
            )
            .add_directory(
                LocationRole::PipCache,
                cache.join("pip"),
                "the XDG pip cache",
            )
            .add_directory(
                LocationRole::GradleCache,
                home.join(".gradle/caches"),
                "the user Gradle caches",
            )
            .add_directory(
                LocationRole::MavenRepository,
                home.join(".m2"),
                "the user Maven .m2 directory",
            )
    }

    pub fn platform_defaults() -> Self {
        #[cfg(windows)]
        {
            Self::windows_defaults()
        }
        #[cfg(target_os = "linux")]
        {
            Self::linux_defaults()
        }
        #[cfg(not(any(windows, target_os = "linux")))]
        {
            Self::new()
        }
    }
}

#[cfg(target_os = "linux")]
use std::env;

fn starts_with_components(path: &Path, prefix: &Path) -> bool {
    let mut path_components = path.components();
    prefix
        .components()
        .all(|component| path_components.next().is_some_and(|next| next == component))
}

#[cfg(windows)]
fn normalise_for_match(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(normal) => {
                result.push(normal.to_string_lossy().to_lowercase());
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}

#[cfg(not(windows))]
fn normalise_for_match(path: &Path) -> PathBuf {
    path.to_path_buf()
}
