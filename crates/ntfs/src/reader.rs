//! Walks every MFT record. The disk is read sequentially in large chunks on one
//! thread; the record parsing runs on a worker pool, and results reach the
//! caller on the calling thread.

use std::{
    cmp::min,
    io::{Read, Seek, SeekFrom},
    sync::{Mutex, mpsc},
    thread,
};

use crate::{BootSector, NtfsError, Record, parse_record};

/// Record number of the root directory (`\`), fixed by the NTFS format.
pub const ROOT_RECORD: u64 = 5;

/// Read size per request. A multiple of every valid record size and cluster size.
const CHUNK_BYTES: usize = 1 << 20;

/// Counts from one pass over the MFT.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReadStats {
    /// Record slots examined, used or not.
    pub slots: u64,
    /// Slots that looked like records but failed to parse; skipped.
    pub bad_records: u64,
}

/// A run of raw MFT bytes, with the number of its first record.
struct Chunk {
    first: u64,
    bytes: Vec<u8>,
}

/// Records parsed from one chunk.
struct Parsed {
    slots: u64,
    bad_records: u64,
    records: Vec<(u64, Record)>,
}

/// Calls `on_record(number, record)` for every in-use `FILE` record in the MFT.
/// Callbacks run on the calling thread, in no guaranteed order.
pub fn read_records<R, F>(
    source: &mut R,
    boot: BootSector,
    mut on_record: F,
) -> Result<ReadStats, NtfsError>
where
    R: Read + Seek + Send,
    F: FnMut(u64, Record),
{
    let record_size =
        usize::try_from(boot.record_size).map_err(|_| NtfsError::BadBootSector("record size"))?;
    let sector = usize::try_from(boot.bytes_per_sector)
        .map_err(|_| NtfsError::BadBootSector("sector size"))?;

    // Record 0 describes $MFT itself, including where the rest of the table lives.
    let mut first = vec![0u8; record_size];
    source.seek(SeekFrom::Start(boot.mft_lcn * boot.cluster_size))?;
    source.read_exact(&mut first)?;
    let mft =
        crate::parse_record(&mut first, sector)?.ok_or(NtfsError::Corrupt("$MFT record 0"))?;
    let runs = mft
        .data_runs
        .ok_or(NtfsError::Corrupt("$MFT has no data runs"))?;
    let mft_size = mft.data.map_or(0, |data| data.logical);
    let total_records = mft_size / boot.record_size;

    let workers = thread::available_parallelism()
        .map_or(4, |cores| cores.get())
        .clamp(2, 16);
    let (work_tx, work_rx) = mpsc::sync_channel::<Chunk>(workers * 2);
    let work_rx = Mutex::new(work_rx);
    let (done_tx, done_rx) = mpsc::channel::<Parsed>();

    thread::scope(|scope| -> Result<ReadStats, NtfsError> {
        // Reader: one sequential pass over the runs. Sparse runs are counted, not read.
        let reader = scope.spawn(move || -> Result<u64, NtfsError> {
            let mut number: u64 = 0;
            let mut sparse_skipped: u64 = 0;
            for run in runs {
                let run_bytes = run.clusters * boot.cluster_size;
                let mut done: u64 = 0;
                while done < run_bytes && number < total_records {
                    let take = min(CHUNK_BYTES as u64, run_bytes - done) as usize;
                    let whole = (take / record_size) as u64;
                    let count = whole.min(total_records - number);
                    match run.lcn {
                        Some(lcn) => {
                            let mut bytes = vec![0u8; take];
                            source.seek(SeekFrom::Start(lcn * boot.cluster_size + done))?;
                            source.read_exact(&mut bytes)?;
                            bytes.truncate(count as usize * record_size);
                            if work_tx
                                .send(Chunk {
                                    first: number,
                                    bytes,
                                })
                                .is_err()
                            {
                                break;
                            }
                        }
                        None => sparse_skipped += count,
                    }
                    number += count;
                    done += take as u64;
                }
            }
            Ok(sparse_skipped)
        });

        for _ in 0..workers {
            let done = done_tx.clone();
            let work = &work_rx;
            scope.spawn(move || worker(work, done, sector, record_size));
        }
        drop(done_tx);

        let mut stats = ReadStats::default();
        for parsed in done_rx {
            stats.slots += parsed.slots;
            stats.bad_records += parsed.bad_records;
            for (number, record) in parsed.records {
                on_record(number, record);
            }
        }
        let sparse_skipped = reader
            .join()
            .map_err(|_| NtfsError::Corrupt("MFT reader stopped unexpectedly"))??;
        stats.slots += sparse_skipped;
        Ok(stats)
    })
}

/// Parses chunks until the reader closes the queue. Only in-use records are sent back.
fn worker(
    work: &Mutex<mpsc::Receiver<Chunk>>,
    done: mpsc::Sender<Parsed>,
    sector: usize,
    record_size: usize,
) {
    loop {
        let chunk = match work.lock() {
            Ok(queue) => queue.recv(),
            Err(_) => break,
        };
        let Ok(mut chunk) = chunk else { break };

        let mut parsed = Parsed {
            slots: 0,
            bad_records: 0,
            records: Vec::new(),
        };
        for (index, slot) in chunk.bytes.chunks_exact_mut(record_size).enumerate() {
            parsed.slots += 1;
            match parse_record(slot, sector) {
                Ok(Some(record)) if record.in_use => {
                    parsed.records.push((chunk.first + index as u64, record));
                }
                Ok(_) => {}
                Err(_) => parsed.bad_records += 1,
            }
        }
        if done.send(parsed).is_err() {
            break;
        }
    }
}
