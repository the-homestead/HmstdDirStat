//! One timed scan with the generic scanner, reduced to a record that can be written as JSON.

use std::{hint::black_box, path::Path, time::Duration, time::Instant};

use serde::{Deserialize, Serialize};
use timberstat_filesystem::NodeId;
use timberstat_scanner::{
    CancelToken, FileSystemScanner, GenericScanner, ScanError, ScanOptions, ScanOutput,
};

use crate::{datasets::Manifest, mem::PeakSampler};

/// Working-set sampling interval used by `perf-scan`. Recorded with every result.
pub const SAMPLE_INTERVAL: Duration = Duration::from_millis(2);

/// Inputs for one run.
pub struct RunPlan<'a> {
    pub root: &'a Path,
    pub options: &'a ScanOptions,
    pub cache_label: &'a str,
    pub run: u32,
    pub manifest: Option<&'a Manifest>,
    /// Sample the working set during the scan. Off measures the sampler's own overhead baseline.
    pub sample_memory: bool,
}

/// What one run measured. `wall_ms` covers the scanner call only, not dataset setup.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunRecord {
    pub dataset: String,
    pub root: String,
    pub run: u32,
    pub cache_label: String,
    pub threads: usize,
    pub method: String,
    pub wall_ms: f64,
    /// Nodes in the tree, root included.
    pub entries: u64,
    pub files: u64,
    /// Directories in the tree, root included.
    pub directories: u64,
    pub issues: u64,
    pub entries_per_sec: f64,
    /// Highest sampled working set of this process during the run, in bytes.
    /// A lower bound on the OS peak; see `mem.rs`.
    pub peak_working_set_bytes: u64,
    pub memory_sample_interval_ms: f64,
    /// Node count from the dataset manifest, when one exists beside the root.
    pub expected_tree_nodes: Option<u64>,
    /// `Some(true)` when the scan found exactly the manifest's node count.
    pub nodes_match_manifest: Option<bool>,
}

/// Runs the generic scanner once under the memory sampler and returns the record.
pub fn execute(plan: &RunPlan<'_>) -> Result<RunRecord, ScanError> {
    let sampler = plan
        .sample_memory
        .then(|| PeakSampler::start(SAMPLE_INTERVAL));
    let cancel = CancelToken::new();
    let started = Instant::now();
    let scanned = GenericScanner.scan(plan.root, plan.options, &cancel, &mut |_| {});
    let wall_ms = started.elapsed().as_secs_f64() * 1_000.0;
    let peak = sampler.map_or(0, PeakSampler::finish);
    let output = black_box(scanned?);
    Ok(build(plan, &output, wall_ms, peak))
}

fn build(plan: &RunPlan<'_>, output: &ScanOutput, wall_ms: f64, peak: u64) -> RunRecord {
    let entries = output.tree.len() as u64;
    let totals = output
        .tree
        .node(NodeId::ROOT)
        .map(|node| node.totals())
        .unwrap_or_default();
    let expected = plan.manifest.map(|m| m.expected_tree_nodes);
    let dataset = plan
        .manifest
        .map(|m| m.dataset.clone())
        .or_else(|| {
            plan.root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
        .unwrap_or_default();
    RunRecord {
        dataset,
        root: plan.root.display().to_string(),
        run: plan.run,
        cache_label: plan.cache_label.to_owned(),
        threads: plan.options.worker_count(),
        method: format!("{:?}", output.report.method),
        wall_ms,
        entries,
        files: totals.files,
        directories: totals.directories,
        issues: output.report.issue_count(),
        entries_per_sec: if wall_ms > 0.0 {
            entries as f64 / (wall_ms / 1_000.0)
        } else {
            0.0
        },
        peak_working_set_bytes: peak,
        memory_sample_interval_ms: if plan.sample_memory {
            SAMPLE_INTERVAL.as_secs_f64() * 1_000.0
        } else {
            0.0
        },
        expected_tree_nodes: expected,
        nodes_match_manifest: expected.map(|e| e == entries),
    }
}
