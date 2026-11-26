//! Cheap prefilter: a digest of the first and last `SAMPLE_BYTES` of a file.
//!
//! Only files of equal size reach this stage, so the digest needs no length
//! prefix. Files too small for two separate windows are read whole once.

use std::fs::File;
use std::io::{self, Seek, SeekFrom};
use std::path::Path;

use crate::hash::{Digest, fill};

/// Length of each sampled window, at the start and at the end of a file.
pub const SAMPLE_BYTES: u64 = 64 * 1024;

/// BLAKE3 over the head window followed by the tail window. The tail starts
/// after the head, so a small file is never read twice.
pub(crate) fn sample_digest(path: &Path, size: u64) -> io::Result<Digest> {
    let window = usize::try_from(SAMPLE_BYTES).unwrap_or(usize::MAX);
    let mut file = File::open(path)?;
    let mut buf = vec![0u8; window];
    let mut hasher = blake3::Hasher::new();

    let head = fill(&mut file, &mut buf)?;
    hasher.update(&buf[..head]);

    let tail_start = size.saturating_sub(SAMPLE_BYTES).max(head as u64);
    file.seek(SeekFrom::Start(tail_start))?;
    let tail = fill(&mut file, &mut buf)?;
    hasher.update(&buf[..tail]);

    Ok(*hasher.finalize().as_bytes())
}
