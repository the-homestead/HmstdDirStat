//! Per-dataset summary of repeated runs: median and range, never the best run.

use serde::Serialize;

use crate::run::RunRecord;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DatasetSummary {
    pub dataset: String,
    pub samples: usize,
    pub expected_tree_nodes: Option<u64>,
    pub all_nodes_match_manifest: Option<bool>,
    pub wall_ms_median: f64,
    pub wall_ms_min: f64,
    pub wall_ms_max: f64,
    pub entries_per_sec_median: f64,
    pub peak_working_set_bytes_median: u64,
    pub peak_working_set_bytes_min: u64,
    pub peak_working_set_bytes_max: u64,
    pub cache_labels: Vec<String>,
}

/// Median of a non-empty slice; the mean of the two middle values for even counts.
pub fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    let n = values.len();
    if n == 0 {
        return 0.0;
    }
    if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    }
}

/// Groups records by dataset and summarises each group. Output is sorted by dataset id.
pub fn summarize(records: &[RunRecord]) -> Vec<DatasetSummary> {
    let mut datasets: Vec<&str> = records.iter().map(|r| r.dataset.as_str()).collect();
    datasets.sort_unstable();
    datasets.dedup();
    datasets
        .into_iter()
        .map(|dataset| {
            let group: Vec<&RunRecord> = records.iter().filter(|r| r.dataset == dataset).collect();
            let mut walls: Vec<f64> = group.iter().map(|r| r.wall_ms).collect();
            let mut rates: Vec<f64> = group.iter().map(|r| r.entries_per_sec).collect();
            let mut peaks: Vec<f64> = group
                .iter()
                .map(|r| r.peak_working_set_bytes as f64)
                .collect();
            let wall_min = walls.iter().copied().fold(f64::INFINITY, f64::min);
            let wall_max = walls.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let peak_min = peaks.iter().copied().fold(f64::INFINITY, f64::min);
            let peak_max = peaks.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let mut cache_labels: Vec<String> =
                group.iter().map(|r| r.cache_label.clone()).collect();
            cache_labels.sort();
            cache_labels.dedup();
            let all_match = group
                .iter()
                .map(|r| r.nodes_match_manifest)
                .try_fold(true, |acc, m| m.map(|ok| acc && ok));
            DatasetSummary {
                dataset: dataset.to_owned(),
                samples: group.len(),
                expected_tree_nodes: group.first().and_then(|r| r.expected_tree_nodes),
                all_nodes_match_manifest: all_match,
                wall_ms_median: median(&mut walls),
                wall_ms_min: wall_min,
                wall_ms_max: wall_max,
                entries_per_sec_median: median(&mut rates),
                peak_working_set_bytes_median: median(&mut peaks) as u64,
                peak_working_set_bytes_min: peak_min as u64,
                peak_working_set_bytes_max: peak_max as u64,
                cache_labels,
            }
        })
        .collect()
}
