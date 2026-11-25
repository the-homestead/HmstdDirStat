use tracing_subscriber::{EnvFilter, fmt};

/// Installs the global tracing subscriber once.
///
/// The level comes from `HMSTD_LOG` (for example `info` or `hmstd_core=debug`)
/// and defaults to `info`. A second call returns an error instead of panicking.
pub fn init() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let filter = EnvFilter::try_from_env("HMSTD_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).with_target(false).try_init()
}
