use timberstat_scan_history::Scan;

use crate::{
    ComparisonError,
    index::{IndexedScan, ensure_compatible_schema},
    model::{GrowthTrend, SizeDelta, TrendInterval, TrendSnapshot},
};

/// Build a root-size trend from at least three snapshots in oldest-to-newest order.
pub fn growth_trend(scans: &[&Scan]) -> Result<GrowthTrend, ComparisonError> {
    const MINIMUM: usize = 3;
    if scans.len() < MINIMUM {
        return Err(ComparisonError::TooFewSnapshots {
            actual: scans.len(),
            minimum: MINIMUM,
        });
    }

    let root = scans[0].root_path().to_path_buf();
    for (index, scan) in scans.iter().enumerate() {
        ensure_compatible_schema(scan.schema_version(), index)?;
        if scan.root_path() != root {
            return Err(ComparisonError::MismatchedRoots {
                snapshot: index,
                expected: root,
                found: scan.root_path().to_path_buf(),
            });
        }
    }

    let mut snapshots = Vec::with_capacity(scans.len());
    for (index, scan) in scans.iter().enumerate() {
        let indexed = IndexedScan::build(scan, index)?;
        snapshots.push(TrendSnapshot {
            timestamp_unix_secs: scan.provenance.timestamp_unix_secs,
            total: indexed.root_total(),
        });
    }
    let intervals = snapshots
        .windows(2)
        .map(|pair| TrendInterval {
            from_timestamp_unix_secs: pair[0].timestamp_unix_secs,
            to_timestamp_unix_secs: pair[1].timestamp_unix_secs,
            delta: SizeDelta::between(pair[0].total, pair[1].total),
        })
        .collect();

    Ok(GrowthTrend {
        root,
        snapshots,
        intervals,
    })
}
