//! Working-set sampling for the running process.
//!
//! The sampler thread reads this process's working set (Windows: `WorkingSetSize`;
//! Linux: resident set) every `interval`. The result is the highest value seen,
//! so it is a lower bound on the OS peak. A short spike between two samples can be
//! missed; the interval is recorded with every result.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

/// Background sampler for this process's memory.
pub struct PeakSampler {
    stop: Arc<AtomicBool>,
    peak: Arc<AtomicU64>,
    handle: Option<JoinHandle<()>>,
}

impl PeakSampler {
    /// Starts sampling. The first sample is taken immediately.
    pub fn start(interval: Duration) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let peak = Arc::new(AtomicU64::new(0));
        let (stop_flag, peak_slot) = (Arc::clone(&stop), Arc::clone(&peak));
        let handle = thread::spawn(move || {
            let pid = Pid::from_u32(std::process::id());
            let mut system = System::new();
            let kind = ProcessRefreshKind::new().with_memory();
            loop {
                system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), kind);
                if let Some(process) = system.process(pid) {
                    peak_slot.fetch_max(process.memory(), Ordering::Relaxed);
                }
                if stop_flag.load(Ordering::Relaxed) {
                    break;
                }
                thread::sleep(interval);
            }
        });
        Self {
            stop,
            peak,
            handle: Some(handle),
        }
    }

    /// Stops sampling and returns the highest working set seen, in bytes.
    pub fn finish(mut self) -> u64 {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        self.peak.load(Ordering::Relaxed)
    }
}
