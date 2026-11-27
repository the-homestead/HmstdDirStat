//! Whole-file BLAKE3 digests and streamed byte comparison.
//!
//! Both read in 1 MiB chunks and check the cancel flag before every chunk, so
//! a cancel is honoured within one chunk read.

use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use crate::progress::Progress;

/// A 32-byte BLAKE3 digest.
pub type Digest = [u8; 32];

/// Read size for full hashing and byte comparison.
pub const CHUNK_BYTES: usize = 1 << 20;

/// Lower-case hex form of a digest, as shown in the panel and in reports.
pub fn digest_hex(digest: &Digest) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Reads until `buf` is full or end of file, retrying interrupted reads.
/// Returns the number of bytes placed in `buf`.
pub(crate) fn fill(file: &mut File, buf: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match file.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(filled)
}

/// BLAKE3 of the whole file. `Ok(None)` means the run was cancelled.
pub(crate) fn full_digest(path: &Path, progress: &Progress) -> io::Result<Option<Digest>> {
    let mut file = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; CHUNK_BYTES];
    loop {
        if progress.is_cancelled() {
            return Ok(None);
        }
        let read = fill(&mut file, &mut buf)?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
        progress.advance(read as u64);
    }
    Ok(Some(*hasher.finalize().as_bytes()))
}

/// Computes one file's BLAKE3 digest with the same streamed, cancellable
/// implementation used by duplicate detection. `Ok(None)` means cancelled.
pub fn hash_file(path: &Path, progress: &Progress) -> io::Result<Option<Digest>> {
    full_digest(path, progress)
}

/// Result of a byte comparison between two files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Compared {
    Same,
    Different,
    Cancelled,
}

/// Compares two files byte for byte. On failure, returns the path that could
/// not be read so the caller records the failure against the right file.
pub(crate) fn compare_bytes(
    first: &Path,
    second: &Path,
    progress: &Progress,
) -> Result<Compared, (PathBuf, io::Error)> {
    let mut a = File::open(first).map_err(|e| (first.to_path_buf(), e))?;
    let mut b = File::open(second).map_err(|e| (second.to_path_buf(), e))?;
    let mut buf_a = vec![0u8; CHUNK_BYTES];
    let mut buf_b = vec![0u8; CHUNK_BYTES];
    loop {
        if progress.is_cancelled() {
            return Ok(Compared::Cancelled);
        }
        let read_a = fill(&mut a, &mut buf_a).map_err(|e| (first.to_path_buf(), e))?;
        let read_b = fill(&mut b, &mut buf_b).map_err(|e| (second.to_path_buf(), e))?;
        if read_a != read_b || buf_a[..read_a] != buf_b[..read_b] {
            return Ok(Compared::Different);
        }
        if read_a == 0 {
            return Ok(Compared::Same);
        }
        progress.advance(read_a as u64);
    }
}
