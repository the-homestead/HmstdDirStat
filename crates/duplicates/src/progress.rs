//! Shared progress and cancellation for one duplicate run. The UI keeps an
//! `Arc<Progress>`, hands a clone to the worker, and polls `snapshot` values.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};

/// The phase a run is in. Shown as the status line of the panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum Stage {
    #[default]
    Idle = 0,
    Sizing = 1,
    Identity = 2,
    Sampling = 3,
    Hashing = 4,
    Verifying = 5,
    Done = 6,
}

impl Stage {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Sizing => "grouping by size",
            Self::Identity => "resolving hard links",
            Self::Sampling => "sampling first and last 64 KiB",
            Self::Hashing => "hashing with BLAKE3",
            Self::Verifying => "verifying bytes",
            Self::Done => "done",
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Sizing,
            2 => Self::Identity,
            3 => Self::Sampling,
            4 => Self::Hashing,
            5 => Self::Verifying,
            6 => Self::Done,
            _ => Self::Idle,
        }
    }
}

/// Cancel flag plus progress counters. `done` and `total` are in the unit of
/// the current stage: entries while sizing, identity and sampling; bytes while
/// hashing and verifying. A determinate bar is shown when `total > 0`.
#[derive(Debug, Default)]
pub struct Progress {
    cancelled: AtomicBool,
    stage: AtomicU8,
    done: AtomicU64,
    total: AtomicU64,
}

impl Progress {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Requests cancellation. Checked between files and between read chunks.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn stage(&self) -> Stage {
        Stage::from_u8(self.stage.load(Ordering::SeqCst))
    }

    pub fn done(&self) -> u64 {
        self.done.load(Ordering::SeqCst)
    }

    pub fn total(&self) -> u64 {
        self.total.load(Ordering::SeqCst)
    }

    pub(crate) fn begin(&self, stage: Stage, total: u64) {
        self.done.store(0, Ordering::SeqCst);
        self.total.store(total, Ordering::SeqCst);
        self.stage.store(stage as u8, Ordering::SeqCst);
    }

    pub(crate) fn advance(&self, by: u64) {
        self.done.fetch_add(by, Ordering::SeqCst);
    }

    pub(crate) fn finish(&self) {
        self.stage.store(Stage::Done as u8, Ordering::SeqCst);
    }
}
