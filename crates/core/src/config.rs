use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use config::{Config, Environment, File, FileFormat};
use serde::{Deserialize, Serialize};

use crate::error::ConfigError;

/// The only configuration schema this build reads or writes.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// File name inside the platform configuration directory.
pub const CONFIG_FILE_NAME: &str = "config.json";

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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppConfig {
    pub schema_version: u32,
    #[serde(default)]
    pub theme: ThemePreference,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            theme: ThemePreference::default(),
        }
    }
}

/// Resolves the default configuration file path for this user.
pub fn default_config_path() -> Result<PathBuf, ConfigError> {
    let dirs = directories::ProjectDirs::from("", "", "HmstdDirStat")
        .ok_or(ConfigError::NoConfigDirectory)?;
    Ok(dirs.config_dir().join(CONFIG_FILE_NAME))
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

/// Persists a theme choice. Reads the file without environment overrides,
/// replaces only the theme, and saves, so a one-run override is never written.
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
            };
            save_to(&path, &config).unwrap();
            assert_eq!(layered_load(&path, no_env()).unwrap(), config);
        }
    }

    #[test]
    fn save_replaces_existing_file_and_leaves_no_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_path(&dir);
        save_to(&path, &AppConfig::default()).unwrap();
        let dark = AppConfig {
            schema_version: CURRENT_SCHEMA_VERSION,
            theme: ThemePreference::Dark,
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
