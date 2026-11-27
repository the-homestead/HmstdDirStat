use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use signal_hook::{SigId, consts::signal::SIGINT};
use timberstat_duplicates::Progress;
use timberstat_scanner::CancelToken;

use crate::error::{CliError, CliResult};

/// Bridges one process-wide Ctrl+C flag to both shared engine cancellation models.
/// The signal action only sets an atomic; ordinary Rust runs on this monitor thread.
pub struct Cancellation {
    interrupted: Arc<AtomicBool>,
    stop_monitor: Arc<AtomicBool>,
    scan: CancelToken,
    duplicates: Arc<Progress>,
    monitor: Option<JoinHandle<()>>,
    // Registration lasts for this short-lived CLI process. SigId has no drop action.
    _registration: SigId,
}

impl Cancellation {
    pub fn install() -> CliResult<Self> {
        let interrupted = Arc::new(AtomicBool::new(false));
        let registration =
            signal_hook::flag::register(SIGINT, interrupted.clone()).map_err(|error| {
                CliError::failure(format!("could not install Ctrl+C handler: {error}"))
            })?;
        let stop_monitor = Arc::new(AtomicBool::new(false));
        let scan = CancelToken::new();
        let duplicates = Progress::new();
        let monitor = spawn_monitor(
            interrupted.clone(),
            stop_monitor.clone(),
            scan.clone(),
            duplicates.clone(),
        );
        Ok(Self {
            interrupted,
            stop_monitor,
            scan,
            duplicates,
            monitor: Some(monitor),
            _registration: registration,
        })
    }

    pub fn scan_token(&self) -> &CancelToken {
        &self.scan
    }

    pub fn duplicate_progress(&self) -> &Arc<Progress> {
        &self.duplicates
    }

    pub fn is_cancelled(&self) -> bool {
        self.interrupted.load(Ordering::Relaxed)
    }

    pub fn check(&self) -> CliResult<()> {
        if self.is_cancelled() {
            Err(CliError::cancelled())
        } else {
            Ok(())
        }
    }
}

impl Drop for Cancellation {
    fn drop(&mut self) {
        self.stop_monitor.store(true, Ordering::Relaxed);
        if let Some(monitor) = self.monitor.take() {
            let _ = monitor.join();
        }
    }
}

fn spawn_monitor(
    interrupted: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    scan: CancelToken,
    duplicates: Arc<Progress>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            if interrupted.load(Ordering::Relaxed) {
                scan.cancel();
                duplicates.cancel();
                eprintln!("cancellation requested; no partial result will be marked complete");
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
    })
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    #[test]
    fn sigint_reaches_tokens_and_cancelled_export_creates_no_file() {
        let cancellation = Cancellation::install().unwrap();
        signal_hook::low_level::raise(SIGINT).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !cancellation.scan_token().is_cancelled() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(cancellation.scan_token().is_cancelled());
        assert!(cancellation.duplicate_progress().is_cancelled());
        assert_eq!(cancellation.check().unwrap_err().code, 130);

        let temporary = tempfile::tempdir().unwrap();
        let destination = temporary.path().join("partial.json");
        let options = crate::args::ExportOptions {
            format: crate::args::Format::Json,
            out: Some(destination.clone()),
            overwrite: false,
        };
        let error = crate::output::emit("partial data", &options, &cancellation).unwrap_err();
        assert_eq!(error.code, 130);
        assert!(!destination.exists());
    }
}
