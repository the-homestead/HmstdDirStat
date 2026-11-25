use std::path::PathBuf;

/// Errors raised by configuration loading and saving.
///
/// Every variant carries enough context to be shown to the user or logged
/// without guessing which file or setting caused it.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Reading or writing the configuration file failed.
    #[error("configuration I/O failed at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Layered loading failed: malformed file, wrong value type, or an
    /// invalid environment override.
    #[error("configuration could not be loaded: {0}")]
    Load(#[from] config::ConfigError),

    /// Serializing the configuration for writing failed.
    #[error("configuration could not be serialized: {source}")]
    Serialize {
        #[source]
        source: serde_json::Error,
    },

    #[error("configuration schema version {found} is not supported (supported: {supported})")]
    UnsupportedVersion { found: u32, supported: u32 },

    #[error("no platform configuration directory is available for this user")]
    NoConfigDirectory,
}
