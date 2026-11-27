//! Criterion microbenchmarks.
//!
//! * `scan_generic/*`: the portable scanner over the small, deep and many-small
//!   datasets from `timberstat_perf::datasets`. These are disk-bound and warm-cache
//!   only; the OS cache state is not controlled here.
//! * `tree_rollup/*`: in-memory `FileTree::roll_up` on synthetic trees, no disk I/O.
//!
//! Datasets are generated under the temp directory on first use. The 100k,
//! 1m and few-huge datasets are left to the macrobenchmark harness.

use std::hint::black_box;

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use timberstat_filesystem::{EntryKind, FileTree, Metadata, NewEntry, NodeId};
use timberstat_perf::datasets::{self, Kind};
use timberstat_scanner::{CancelToken, FileSystemScanner, GenericScanner, ScanOptions};

fn bench_scan(c: &mut Criterion) {
    let mut group = c.benchmark_group("scan_generic");
    group.sample_size(10);
    for kind in [Kind::Small, Kind::Deep, Kind::ManySmall] {
        let (root, manifest) = datasets::ensure(kind).expect("dataset available");
        group.throughput(Throughput::Elements(manifest.expected_tree_nodes));
        group.bench_with_input(BenchmarkId::from_parameter(kind.id()), &root, |b, root| {
            b.iter(|| {
                let out = GenericScanner
                    .scan(
                        black_box(root),
                        &ScanOptions::default(),
                        &CancelToken::new(),
                        &mut |_| {},
                    )
                    .expect("scan");
                black_box(out.tree.len())
            });
        });
    }
    group.finish();
}

const FANOUT: usize = 16;

fn file_entry(i: usize) -> NewEntry {
    NewEntry {
        name: format!("f{i}").into(),
        kind: EntryKind::File,
        logical_size: (i as u64 * 37) % 4096,
        physical_size: Some(4096),
        identity: None,
        hardlink_duplicate: false,
        unreadable: false,
        metadata: Metadata::default(),
    }
}

/// A balanced tree of `nodes` nodes with `FANOUT` children per directory.
fn synthetic_tree(nodes: usize) -> FileTree {
    let mut tree = FileTree::with_root(NewEntry::directory("root".into(), Metadata::default()));
    let mut ids: Vec<NodeId> = vec![NodeId::ROOT];
    for i in 1..nodes {
        let parent = ids[(i - 1) / FANOUT];
        let id = tree.add_child(parent, file_entry(i)).expect("add child");
        ids.push(id);
    }
    tree
}

fn bench_rollup(c: &mut Criterion) {
    let mut group = c.benchmark_group("tree_rollup");
    for nodes in [10_000usize, 100_000] {
        group.throughput(Throughput::Elements(nodes as u64));
        group.bench_with_input(BenchmarkId::from_parameter(nodes), &nodes, |b, &nodes| {
            b.iter_batched(
                || synthetic_tree(nodes),
                |mut tree| {
                    tree.roll_up();
                    black_box(tree)
                },
                BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

criterion_group!(benches, bench_scan, bench_rollup);
criterion_main!(benches);
