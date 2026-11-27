//! Whether a drive is a spinning hard disk. The native query lives in `timberstat-diskprobe`;
//! this wraps it with the timer line the CLI and the GUI both print. Any failure is "not a
//! spinning disk", so the drive is walked.

use std::path::Path;
use std::time::Instant;

/// True when the drive's device reports a seek penalty, which only spinning disks do.
pub fn is_spinning_hdd(drive: &Path) -> bool {
    let started = Instant::now();
    let answer = crate::drive_letter(drive)
        .and_then(timberstat_diskprobe::seek_penalty)
        .unwrap_or(false);
    tracing::info!(
        "{}",
        timberstat_core::timing::line("detect drive kind", started.elapsed())
    );
    answer
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn non_drive_paths_are_never_spinning_disks() {
        assert!(!is_spinning_hdd(Path::new("/home/user")));
        assert!(!is_spinning_hdd(Path::new("")));
    }
}
