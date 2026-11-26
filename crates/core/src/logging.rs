//! Tracing setup. Each run writes one log file into the data directory, and the
//! newest [`KEEP_LOG_FILES`] are kept. The release build has no console window,
//! so the file is the only place messages go.

use std::{
    error::Error,
    fs::{self, OpenOptions},
    io,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use tracing_subscriber::{EnvFilter, fmt};

/// Log files kept on disk, including the one written by the current run.
pub const KEEP_LOG_FILES: usize = 20;

/// Directory for log files, e.g. `%LOCALAPPDATA%\TimberStat\data\logs` on Windows.
pub fn log_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", crate::config::APP_NAME)
        .map(|dirs| dirs.data_local_dir().join("logs"))
}

/// Creates a new log file in `dir`, prunes older ones, and installs the global subscriber.
///
/// The level comes from `HMSTD_LOG` (for example `info` or `timberstat_core=debug`) and
/// defaults to `info`. Returns the path of the file written by this run.
pub fn init(dir: &Path) -> Result<PathBuf, Box<dyn Error + Send + Sync>> {
    fs::create_dir_all(dir)?;
    // Leave room for the file this run is about to create.
    prune(dir, KEEP_LOG_FILES - 1)?;

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let path = dir.join(format!("timberstat-{stamp:012}.log"));
    let file = OpenOptions::new().create(true).append(true).open(&path)?;

    let filter = EnvFilter::try_from_env("HMSTD_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_ansi(false)
        .with_writer(Mutex::new(file))
        .try_init()?;
    Ok(path)
}

/// Deletes the oldest `.log` files in `dir` so that at most `keep` remain.
///
/// File names carry a fixed-width timestamp, so sorting by name sorts by age.
pub fn prune(dir: &Path, keep: usize) -> io::Result<()> {
    let mut logs: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "log"))
        .collect();
    logs.sort();
    let excess = logs.len().saturating_sub(keep);
    for old in logs.into_iter().take(excess) {
        // A file that cannot be removed (in use, say) is left for the next run.
        let _ = fs::remove_file(old);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prune_keeps_the_newest_logs() {
        let dir = tempfile::tempdir().unwrap();
        for stamp in 1..=5 {
            fs::write(dir.path().join(format!("timberstat-{stamp:012}.log")), "").unwrap();
        }
        fs::write(dir.path().join("notes.txt"), "").unwrap();

        prune(dir.path(), 2).unwrap();

        let mut left: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        // Sorted by name, like `left`: "notes.txt" precedes the "timberstat-" logs.
        assert_eq!(
            left,
            vec![
                "notes.txt".to_string(),
                "timberstat-000000000004.log".to_string(),
                "timberstat-000000000005.log".to_string(),
            ]
        );
    }
}
