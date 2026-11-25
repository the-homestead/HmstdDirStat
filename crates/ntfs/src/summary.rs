//! One-call volume summary for the elevated Fast scan helper, and a plain-text
//! format the unelevated app can read back.

use std::{fs::File, io::Read, time::Instant};

use hmstd_filesystem::NodeId;

use crate::{Catalog, NtfsError, parse_boot_sector, read_records_with_progress};

/// Totals for one NTFS volume, read from its Master File Table.
#[derive(Debug, Clone, PartialEq)]
pub struct VolumeSummary {
    pub volume: String,
    pub files: u64,
    pub directories: u64,
    pub logical: u64,
    pub physical: u64,
    pub complete: bool,
    pub orphans: u64,
    pub bad_records: u64,
    pub read_secs: f64,
    pub total_secs: f64,
}

/// What a Fast read is doing right now, for progress display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummaryProgress {
    /// Reading MFT records: `done` of `total` slots examined so far.
    Reading { done: u64, total: u64 },
    /// All records are read; the folder tree is being built.
    Building,
}

/// Reads and summarises a raw volume such as `//./C:`. Needs administrator rights.
pub fn summarize_volume(volume: &str) -> Result<VolumeSummary, NtfsError> {
    summarize_volume_with_progress(volume, |_| {})
}

/// Same as [`summarize_volume`], and reports [`SummaryProgress`] as it goes.
pub fn summarize_volume_with_progress<P: FnMut(SummaryProgress)>(
    volume: &str,
    mut progress: P,
) -> Result<VolumeSummary, NtfsError> {
    let started = Instant::now();
    let mut file = File::open(volume)?;

    let mut sector = [0u8; 512];
    file.read_exact(&mut sector)?;
    let boot = parse_boot_sector(&sector)?;

    let mut catalog = Catalog::new();
    let stats = read_records_with_progress(
        &mut file,
        boot,
        |number, record| catalog.insert(number, record),
        |done, total| progress(SummaryProgress::Reading { done, total }),
    )?;
    let read_secs = started.elapsed().as_secs_f64();

    progress(SummaryProgress::Building);
    let letter = volume.chars().rev().nth(1).unwrap_or('?');
    let (tree, build) = catalog.into_tree(&format!("{letter}:"))?;
    let totals = tree
        .node(NodeId::ROOT)
        .ok_or(NtfsError::Corrupt("empty tree"))?
        .totals();

    Ok(VolumeSummary {
        volume: volume.to_string(),
        files: totals.files,
        directories: totals.directories,
        logical: totals.logical,
        physical: if totals.physical_known {
            totals.physical
        } else {
            0
        },
        complete: totals.complete && stats.bad_records == 0,
        orphans: build.orphans,
        bad_records: stats.bad_records,
        read_secs,
        total_secs: started.elapsed().as_secs_f64(),
    })
}

impl VolumeSummary {
    /// `key=value` lines, one per field, for the helper process to write.
    pub fn to_text(&self) -> String {
        format!(
            "volume={}\nfiles={}\ndirectories={}\nlogical={}\nphysical={}\ncomplete={}\norphans={}\nbad_records={}\nread_secs={:.3}\ntotal_secs={:.3}\n",
            self.volume,
            self.files,
            self.directories,
            self.logical,
            self.physical,
            self.complete,
            self.orphans,
            self.bad_records,
            self.read_secs,
            self.total_secs
        )
    }

    /// Parses the output of [`to_text`](Self::to_text). Returns `None` if any field is missing.
    pub fn from_text(text: &str) -> Option<Self> {
        let field = |key: &str| {
            text.lines()
                .find_map(|line| line.strip_prefix(key)?.strip_prefix('='))
        };
        Some(Self {
            volume: field("volume")?.to_string(),
            files: field("files")?.parse().ok()?,
            directories: field("directories")?.parse().ok()?,
            logical: field("logical")?.parse().ok()?,
            physical: field("physical")?.parse().ok()?,
            complete: field("complete")?.parse().ok()?,
            orphans: field("orphans")?.parse().ok()?,
            bad_records: field("bad_records")?.parse().ok()?,
            read_secs: field("read_secs")?.parse().ok()?,
            total_secs: field("total_secs")?.parse().ok()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_round_trips() {
        let summary = VolumeSummary {
            volume: "//./C:".into(),
            files: 5,
            directories: 2,
            logical: 1024,
            physical: 4096,
            complete: true,
            orphans: 1,
            bad_records: 0,
            read_secs: 1.5,
            total_secs: 2.25,
        };
        assert_eq!(VolumeSummary::from_text(&summary.to_text()), Some(summary));
    }

    #[test]
    fn missing_field_is_rejected() {
        assert_eq!(VolumeSummary::from_text("files=1\n"), None);
    }
}
