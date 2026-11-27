use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use crate::scan_mode::ScanModeChoice;
use config::{Config, Environment, File, FileFormat};
use serde::{Deserialize, Serialize};

use crate::error::ConfigError;
use crate::workspace::WorkspaceLayout;

/// Smallest and largest UI scale a config file may ask for. Values outside are clamped.
pub const UI_SCALE_MIN: f32 = 0.6;
pub const UI_SCALE_MAX: f32 = 1.5;
/// Starting point: smaller than GPUI's 16px rem, since the default text read too large.
pub const DEFAULT_UI_SCALE: f32 = 0.85;

/// Clamps a UI scale to the supported range. A non-finite value falls back to the default.
pub fn clamp_ui_scale(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(UI_SCALE_MIN, UI_SCALE_MAX)
    } else {
        DEFAULT_UI_SCALE
    }
}

/// The only configuration schema this build reads or writes.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// File name inside the platform configuration directory.
pub const CONFIG_FILE_NAME: &str = "config.json";

/// The product name. It names the per-user config, data and log folders.
pub const APP_NAME: &str = "TimberStat";
/// The name before the rename. Its config is copied over once (see [`adopt_legacy_config`]).
const LEGACY_APP_NAME: &str = "HmstdDirStat";

/// Environment variable prefix. `HMSTD_THEME=dark` overrides the file for one run.
pub const ENV_PREFIX: &str = "HMSTD";

/// Theme preference persisted across runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemePreference {
    /// Follow the operating system appearance.
    #[default]
    System,
    Light,
    Dark,
}

/// Versioned application configuration.
///
/// Unknown fields are ignored so that a newer file written by a future build
/// within the same schema version does not break an older one.
//
//@ ADR: archive-preferred-handler-config
// Decision: introduce `archive.preferred_handler` under `config.json`
// (section `archive`) with value = handler family id string ("sevenzip",
// "tar", ...). Scope is per-user (each user gets their own config file).
// UI calls `ArchiveManager::with_preferred(config.archive.preferred_handler)`.
// Missing preferred handler → hard error (`PreferredHandlerMissing`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppConfig {
    pub schema_version: u32,
    #[serde(default)]
    pub theme: ThemePreference,
    /// Scales text and rem-based spacing. Set in config.json or with `HMSTD_UI_SCALE`.
    #[serde(default = "default_ui_scale")]
    pub ui_scale: f32,
    /// The widget layout last saved. `None` until the user changes it.
    #[serde(default)]
    pub workspace: Option<WorkspaceLayout>,
    /// Preference stored in `config.json` under `archive.preferred_handler`.
    ///
    /// Values are `HandlerFamily::id()` strings ("sevenzip", "tar", "unzip", ...).
    /// `None` means automatic handler selection (7-Zip, then tar, then unzip, ...).
    ///
    /// Scope is per-user (the config file is per-user); there is no per-profile layer.
    /// UI code calls `ArchiveManager::with_preferred(config.archive.preferred_handler)`
    /// to apply it; a missing preferred handler is a hard error (`PreferredHandlerMissing`).
    #[serde(default)]
    pub preferred_handler: Option<String>,
    /// Per-drive scan mode, keyed by drive letter (`C`). Drives without an entry use `Auto`.
    #[serde(default)]
    pub drive_modes: std::collections::BTreeMap<String, crate::scan_mode::ScanModeChoice>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            theme: ThemePreference::default(),
            ui_scale: DEFAULT_UI_SCALE,
            workspace: None,
            preferred_handler: None,
            drive_modes: Default::default(),
        }
    }
}

/// Resolves the default configuration file path for this user.
pub fn default_config_path() -> Result<PathBuf, ConfigError> {
    let dirs =
        directories::ProjectDirs::from("", "", APP_NAME).ok_or(ConfigError::NoConfigDirectory)?;
    Ok(dirs.config_dir().join(CONFIG_FILE_NAME))
}

/// Where the config lived before the rename, if this platform has a config folder.
pub fn legacy_config_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", LEGACY_APP_NAME)
        .map(|dirs| dirs.config_dir().join(CONFIG_FILE_NAME))
}

/// Copies the pre-rename config to `new` when `new` does not exist yet, so the saved layout,
/// theme and UI scale survive the rename. The old file is only read, never changed or
/// removed. Returns true when a copy was made.
pub fn adopt_legacy_config(legacy: &Path, new: &Path) -> std::io::Result<bool> {
    if new.exists() || !legacy.is_file() {
        return Ok(false);
    }
    if let Some(parent) = new.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(legacy, new)?;
    Ok(true)
}

/// Builds the environment layer from an explicit variable map.
///
/// Keys are `HMSTD_<FIELD>`; nested fields would use `__` as separator.
fn env_source(vars: std::collections::HashMap<String, String>) -> Environment {
    Environment::with_prefix(ENV_PREFIX)
        // config-rs otherwise reuses `separator` as the prefix separator,
        // which would expect `HMSTD__THEME` instead of `HMSTD_THEME`.
        .prefix_separator("_")
        .separator("__")
        .source(Some(vars))
}

/// Layers defaults, an optional JSON file, and optional environment overrides
/// with config-rs, then validates the schema version.
fn layered_load(path: &Path, env: Option<Environment>) -> Result<AppConfig, ConfigError> {
    let mut builder = Config::builder()
        .set_default("schema_version", i64::from(CURRENT_SCHEMA_VERSION))?
        .set_default("theme", "system")?
        .set_default("ui_scale", f64::from(DEFAULT_UI_SCALE))?
        .add_source(File::from(path).format(FileFormat::Json).required(false));
    if let Some(env) = env {
        builder = builder.add_source(env);
    }
    let config: AppConfig = builder.build()?.try_deserialize()?;

    if config.schema_version != CURRENT_SCHEMA_VERSION {
        return Err(ConfigError::UnsupportedVersion {
            found: config.schema_version,
            supported: CURRENT_SCHEMA_VERSION,
        });
    }
    Ok(config)
}

/// Loads the effective configuration: defaults, then `path` if present, then
/// `HMSTD_*` environment variables.
///
/// A missing file is not an error. A present file that fails to parse, has a
/// wrong value type, or has an unsupported version is rejected; it is never
/// rewritten by a read.
pub fn load_from(path: &Path) -> Result<AppConfig, ConfigError> {
    layered_load(path, Some(env_source(std::env::vars().collect())))
}

/// Saves configuration to `path` atomically.
///
/// The document is written to a sibling temporary file, flushed to disk, and
/// then renamed over the target. If a step before the rename fails, the
/// previous file is left intact.
pub fn save_to(path: &Path, config: &AppConfig) -> Result<(), ConfigError> {
    let io_err = |source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_err)?;
    }

    let mut json =
        serde_json::to_string_pretty(config).map_err(|source| ConfigError::Serialize { source })?;
    json.push('\n');

    let tmp = path.with_extension("json.tmp");
    let result = (|| -> std::io::Result<()> {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)
    })();

    if let Err(source) = result {
        let _ = fs::remove_file(&tmp);
        return Err(io_err(source));
    }
    Ok(())
}

/// Persists the widget layout. Reads the file without environment overrides, replaces only the
/// workspace entry, and saves.
pub fn store_workspace_layout(path: &Path, layout: &WorkspaceLayout) -> Result<(), ConfigError> {
    let mut config = layered_load(path, None)?;
    config.workspace = Some(layout.clone());
    save_to(path, &config)
}

/// Persists a theme choice. Reads the file without environment overrides,
/// replaces only the theme, and saves, so a one-run override is never written.
/// Saves the UI scale, clamped to the supported range.
pub fn store_ui_scale(path: &Path, scale: f32) -> Result<(), ConfigError> {
    let mut config = layered_load(path, None)?;
    config.ui_scale = clamp_ui_scale(scale);
    save_to(path, &config)
}

/// Saves one drive's scan mode. `drive` is its letter, such as `C`.
pub fn store_drive_mode(
    path: &Path,
    drive: char,
    choice: ScanModeChoice,
) -> Result<(), ConfigError> {
    let mut config = layered_load(path, None)?;
    config
        .drive_modes
        .insert(drive.to_ascii_uppercase().to_string(), choice);
    save_to(path, &config)
}

pub fn store_theme_preference(path: &Path, theme: ThemePreference) -> Result<(), ConfigError> {
    let mut config = layered_load(path, None)?;
    config.theme = theme;
    save_to(path, &config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn temp_path(dir: &tempfile::TempDir) -> PathBuf {
        dir.path().join("nested").join(CONFIG_FILE_NAME)
    }

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn no_env() -> Option<Environment> {
        Some(env_source(HashMap::new()))
    }

    #[test]
    fn legacy_config_is_copied_once_and_left_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("old").join(CONFIG_FILE_NAME);
        let new = dir.path().join("new").join(CONFIG_FILE_NAME);
        write(&legacy, "{\"theme\":\"dark\"}");

        assert!(adopt_legacy_config(&legacy, &new).unwrap());
        assert_eq!(fs::read_to_string(&new).unwrap(), "{\"theme\":\"dark\"}");
        assert!(legacy.is_file());

        // A config that already exists is never replaced.
        write(&legacy, "{\"theme\":\"light\"}");
        assert!(!adopt_legacy_config(&legacy, &new).unwrap());
        assert_eq!(fs::read_to_string(&new).unwrap(), "{\"theme\":\"dark\"}");
    }

    #[test]
    fn nothing_to_adopt_without_a_legacy_file() {
        let dir = tempfile::tempdir().unwrap();
        let new = dir.path().join(CONFIG_FILE_NAME);
        assert!(!adopt_legacy_config(&dir.path().join("missing.json"), &new).unwrap());
        assert!(!new.exists());
    }

    #[test]
    fn defaults_use_current_schema_and_system_theme() {
        let config = AppConfig::default();
        assert_eq!(config.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(config.theme, ThemePreference::System);
    }

    #[test]
    fn missing_file_yields_defaults_without_creating_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_path(&dir);
        assert_eq!(layered_load(&path, no_env()).unwrap(), AppConfig::default());
        assert!(!path.exists());
    }

    #[test]
    fn save_then_load_round_trips_every_theme() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_path(&dir);
        for theme in [
            ThemePreference::System,
            ThemePreference::Light,
            ThemePreference::Dark,
        ] {
            let config = AppConfig {
                schema_version: CURRENT_SCHEMA_VERSION,
                theme,
                ui_scale: DEFAULT_UI_SCALE,
                workspace: None,
                preferred_handler: None,
                drive_modes: Default::default(),
            };
            save_to(&path, &config).unwrap();
            assert_eq!(layered_load(&path, no_env()).unwrap(), config);
        }
    }

    #[test]
    fn workspace_layout_round_trips_and_keeps_the_theme() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_path(&dir);
        store_theme_preference(&path, ThemePreference::Dark).unwrap();
        let mut layout = WorkspaceLayout::default();
        layout.hide(crate::workspace::WidgetKind::Pie);
        store_workspace_layout(&path, &layout).unwrap();
        let loaded = layered_load(&path, no_env()).unwrap();
        assert_eq!(loaded.theme, ThemePreference::Dark);
        assert_eq!(loaded.workspace, Some(layout));
    }

    #[test]
    fn save_replaces_existing_file_and_leaves_no_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_path(&dir);
        save_to(&path, &AppConfig::default()).unwrap();
        let dark = AppConfig {
            schema_version: CURRENT_SCHEMA_VERSION,
            theme: ThemePreference::Dark,
            ui_scale: DEFAULT_UI_SCALE,
            workspace: None,
            preferred_handler: None,
            drive_modes: Default::default(),
        };
        save_to(&path, &dark).unwrap();
        assert_eq!(layered_load(&path, no_env()).unwrap(), dark);
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn unknown_fields_are_tolerated_within_the_same_schema() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_path(&dir);
        write(
            &path,
            r#"{"schema_version":1,"theme":"dark","future_setting":42}"#,
        );
        assert_eq!(
            layered_load(&path, no_env()).unwrap().theme,
            ThemePreference::Dark
        );
    }

    #[test]
    fn missing_theme_field_defaults_to_system() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_path(&dir);
        write(&path, r#"{"schema_version":1}"#);
        assert_eq!(
            layered_load(&path, no_env()).unwrap().theme,
            ThemePreference::System
        );
    }

    #[test]
    fn corrupt_file_is_rejected_and_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_path(&dir);
        write(&path, "{ not json");
        assert!(matches!(
            layered_load(&path, no_env()),
            Err(ConfigError::Load(_))
        ));
        assert_eq!(fs::read_to_string(&path).unwrap(), "{ not json");
    }

    #[test]
    fn unsupported_version_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_path(&dir);
        write(&path, r#"{"schema_version":99,"theme":"light"}"#);
        match layered_load(&path, no_env()) {
            Err(ConfigError::UnsupportedVersion { found, supported }) => {
                assert_eq!(found, 99);
                assert_eq!(supported, CURRENT_SCHEMA_VERSION);
            }
            other => panic!("expected UnsupportedVersion, got {other:?}"),
        }
    }

    #[test]
    fn unknown_theme_value_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_path(&dir);
        write(&path, r#"{"schema_version":1,"theme":"neon"}"#);
        assert!(matches!(
            layered_load(&path, no_env()),
            Err(ConfigError::Load(_))
        ));
    }

    #[test]
    fn environment_overrides_file_for_the_run_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_path(&dir);
        write(&path, r#"{"schema_version":1,"theme":"light"}"#);
        let mut vars = HashMap::new();
        vars.insert("HMSTD_THEME".to_string(), "dark".to_string());
        let config = layered_load(&path, Some(env_source(vars))).unwrap();
        assert_eq!(config.theme, ThemePreference::Dark);
        assert_eq!(
            layered_load(&path, no_env()).unwrap().theme,
            ThemePreference::Light
        );
    }

    #[test]
    fn store_theme_preference_updates_only_theme_and_never_persists_env() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_path(&dir);
        store_theme_preference(&path, ThemePreference::Light).unwrap();
        assert_eq!(
            layered_load(&path, no_env()).unwrap().theme,
            ThemePreference::Light
        );
        store_theme_preference(&path, ThemePreference::Dark).unwrap();
        let config = layered_load(&path, no_env()).unwrap();
        assert_eq!(config.theme, ThemePreference::Dark);
        assert_eq!(config.schema_version, CURRENT_SCHEMA_VERSION);
    }
}

fn default_ui_scale() -> f32 {
    DEFAULT_UI_SCALE
}

#[cfg(test)]
mod ui_scale_tests {
    use super::*;

    #[test]
    fn ui_scale_is_clamped_and_non_finite_falls_back() {
        assert_eq!(clamp_ui_scale(1.0), 1.0);
        assert_eq!(clamp_ui_scale(0.1), UI_SCALE_MIN);
        assert_eq!(clamp_ui_scale(9.0), UI_SCALE_MAX);
        assert_eq!(clamp_ui_scale(f32::NAN), DEFAULT_UI_SCALE);
    }

    #[test]
    fn missing_ui_scale_uses_the_default() {
        let config: AppConfig = serde_json::from_str(r#"{"schema_version":1}"#).unwrap();
        assert_eq!(config.ui_scale, DEFAULT_UI_SCALE);
    }
}
