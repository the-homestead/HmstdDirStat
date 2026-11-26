//! Known locations matched against paths already present in the scanned tree.
//! Registering or matching a location performs no filesystem access.

use std::path::{Path, PathBuf};

#[cfg(windows)]
use std::{env, path::Component};

/// Role assigned to a known directory and its descendants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocationRole {
    Temporary,
    ApplicationCache,
    Logs,
    Downloads,
    CrashDumps,
    GameInstalls,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownLocation {
    pub role: LocationRole,
    /// Case-folded on Windows; unchanged on other platforms.
    pub match_path: PathBuf,
    pub display_path: PathBuf,
    pub label: String,
}

/// Configurable known locations. Tests and callers can supply synthetic roots.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocationSet {
    locations: Vec<KnownLocation>,
}

impl LocationSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(
        mut self,
        role: LocationRole,
        path: impl Into<PathBuf>,
        label: impl Into<String>,
    ) -> Self {
        let display_path = path.into();
        let match_path = normalise_for_match(&display_path);
        if !self
            .locations
            .iter()
            .any(|existing| existing.role == role && existing.match_path == match_path)
        {
            self.locations.push(KnownLocation {
                role,
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

    /// Longest matching registered prefix, regardless of role.
    pub fn match_path(&self, path: &Path) -> Option<&KnownLocation> {
        self.best_match(path, None)
    }

    /// Longest matching prefix restricted to `role`.
    pub fn match_role(&self, path: &Path, role: LocationRole) -> Option<&KnownLocation> {
        self.best_match(path, Some(role))
    }

    fn best_match(&self, path: &Path, role: Option<LocationRole>) -> Option<&KnownLocation> {
        let needle = normalise_for_match(path);
        self.locations
            .iter()
            .filter(|location| role.is_none_or(|wanted| location.role == wanted))
            .filter(|location| starts_with_components(&needle, &location.match_path))
            .max_by_key(|location| location.match_path.components().count())
    }

    /// A direct child of a game-library root represents one installation.
    pub(crate) fn game_installation(&self, path: &Path) -> Option<&KnownLocation> {
        let needle = normalise_for_match(path);
        self.locations.iter().find(|location| {
            location.role == LocationRole::GameInstalls
                && needle.parent() == Some(location.match_path.as_path())
        })
    }

    #[cfg(windows)]
    pub fn windows_defaults() -> Self {
        let mut locations = Self::new();
        for (variable, label) in [
            ("TEMP", "the TEMP temporary directory"),
            ("TMP", "the TMP temporary directory"),
        ] {
            if let Some(path) = env::var_os(variable) {
                locations = locations.add(LocationRole::Temporary, PathBuf::from(path), label);
            }
        }

        if let Some(local) = env::var_os("LOCALAPPDATA").map(PathBuf::from) {
            locations = locations.add(
                LocationRole::Temporary,
                local.join("Temp"),
                "the LOCALAPPDATA\\Temp temporary directory",
            );
            locations = locations.add(
                LocationRole::CrashDumps,
                local.join("CrashDumps"),
                "the user CrashDumps directory",
            );
            locations = locations.add(
                LocationRole::ApplicationCache,
                local.join("Microsoft/Windows/INetCache"),
                "the Windows Internet cache directory",
            );
        }
        if let Some(program_data) = env::var_os("ProgramData").map(PathBuf::from) {
            locations = locations.add(
                LocationRole::CrashDumps,
                program_data.join("Microsoft/Windows/WER"),
                "the Windows Error Reporting directory",
            );
        }
        if let Some(user) = env::var_os("USERPROFILE").map(PathBuf::from) {
            locations = locations.add(
                LocationRole::Downloads,
                user.join("Downloads"),
                "the user Downloads directory",
            );
        }
        for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(program_files) = env::var_os(variable).map(PathBuf::from) {
                locations = locations.add(
                    LocationRole::GameInstalls,
                    program_files.join("Steam/steamapps/common"),
                    "a Steam game library",
                );
            }
        }
        for path in [r"C:\Windows\Logs", r"C:\Windows\System32\LogFiles"] {
            locations = locations.add(
                LocationRole::Logs,
                Path::new(path),
                "a Windows system log directory",
            );
        }
        locations
    }

    #[cfg(target_os = "linux")]
    pub fn linux_defaults() -> Self {
        let mut locations = Self::new()
            .add(
                LocationRole::Temporary,
                "/tmp",
                "the /tmp temporary directory",
            )
            .add(
                LocationRole::Temporary,
                "/var/tmp",
                "the /var/tmp temporary directory",
            )
            .add(LocationRole::Logs, "/var/log", "the /var/log directory")
            .add(
                LocationRole::CrashDumps,
                "/var/lib/systemd/coredump",
                "the systemd core dump directory",
            )
            .add(
                LocationRole::CrashDumps,
                "/var/crash",
                "the system crash dump directory",
            );

        if let Some(base) = directories::BaseDirs::new() {
            locations = locations.add(
                LocationRole::ApplicationCache,
                base.cache_dir(),
                "the XDG cache directory",
            );
            let home = base.home_dir();
            locations = locations.add(
                LocationRole::Downloads,
                home.join("Downloads"),
                "the user Downloads directory",
            );
            for library in [
                home.join(".steam/steam/steamapps/common"),
                home.join(".local/share/Steam/steamapps/common"),
            ] {
                locations =
                    locations.add(LocationRole::GameInstalls, library, "a Steam game library");
            }
        }
        locations
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
