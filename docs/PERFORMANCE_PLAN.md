# PERFORMANCE PLAN — TimberStat

Rule (Plan §18, §23.10, §23.18): **no invented benchmark numbers; profile before optimising.**
This file defines how performance is measured. It contains no throughput claims. No benchmark
result exists yet.

## 1. Metrics

Scanning: scan startup, enumeration throughput (entries/s, directories/s), metadata processing
time, peak RAM, CPU utilisation, time to first visible result.
UI: frame responsiveness during scan, treemap and radial generation time, rendering time, search latency.
Later: hash throughput, file operation throughput.

## 2. Measurement tiers

| Tier | Tool | Scope | CI |
|---|---|---|---|
| Microbench | Criterion 0.8.2 (added in Phase 3) | treemap layout, hashing, diff, serialisation | informational |
| Macrobench | custom harness (`benchmarks/`) | real filesystem scan, end-to-end | manual / scheduled |
| Memory | per-OS profiler (candidates: Windows Performance Analyzer / `heaptrack` on Linux) | peak RAM, allocations | manual |

Criterion is not added before Phase 3 because nothing to measure exists in Phase 1.

## 3. Datasets

Generated, reproducible fixtures (script in `scripts/`, Phase 2):

| Dataset | Shape |
|---|---|
| small | few hundred entries, flat |
| deep | nesting depth > 200 |
| many-small | large count of 0–4 KB files |
| few-large | small count of multi-GB sparse files |
| large-scale | 10⁵ → 10⁶ → 10⁷ entries, only when disk allows |

Each run records: hardware, OS build, filesystem, privilege level, cache state (cold/warm),
dataset id and size distribution, release profile, commit hash, number of samples.

## 4. Method

- Release profile only; `black_box` on results.
- Cold-cache and warm-cache runs reported separately; cold requires a documented cache flush or reboot.
- At least 5 samples; report median and spread, not best run.
- Same-machine baselines only. Cross-machine comparisons are not claims.
- Results committed to `BENCHMARKS.md` with the raw output file, never from memory.

## 5. Architectural budgets (design rules, not measured claims)

- UI thread never performs filesystem I/O (Plan §23.1).
- Bounded worker count and bounded channels; batched UI updates; no unbounded queues.
- Visible primitive cap for layouts (folding small items) so no one-element-per-file.
- View changes never rescan; they re-derive from the model (ARCHITECTURE §1).
- Incremental size aggregation during scan rather than a second full pass.
- Compact `NodeId(u32)` arena; no per-node heap object beyond the name.

These become testable only after Phase 2–3 measurements exist. Until then they are targets.

## 6. CI policy

- Benchmarks run as informational jobs on shared runners with `continue-on-error`.
- No hard performance gate until a stable self-hosted baseline exists.
- A regression is judged against a same-runner baseline, never against a number in a document.

## 7. Phase 1 performance position

Phase 1 is a shell. The only timing recorded is the build and test duration in CI. No throughput,
RAM or responsiveness claim is made.

## 8. Measured scale checks (2026-11-27, Windows x86_64, release)

Measured by the ignored tests `timberstat-filesystem::scale_tests` and
`timberstat-scan-comparison/tests/scale.rs`. Run them with `--release -- --ignored --nocapture`.
Not a benchmark suite: one run each, no repetition, no baseline.

| Measurement | Value |
| --- | --- |
| `size_of::<Node>()` | 184 bytes |
| `size_of::<Option<NodeId>>()` | 8 bytes |
| `size_of::<Option<FileIdentity>>()` | 24 bytes |
| `FileTree` build, 1,001,001 entries (1,000 dirs × 1,000 files) | 639 ms |
| `FileTree::roll_up` on the same tree | 83 ms |
| Node arena for that tree (`184 × entries`, names excluded) | 175 MiB |
| `scan_comparison::compare`, two 1,001,001-entry scans, one file changed | 10.7 s |

The compare figure is the largest outlier and is not yet explained. `scan-comparison/src/index.rs`
builds a `HashMap<PathBuf, _>` per scan, and `compare` performs a lookup per entry in each
direction. Each `PathBuf` is a fresh allocation. Profile before changing it.

## 9. Measured vs. budget

Section 5 lists design budgets. The measured Node size has no agreed budget to compare against. Decide one
before tightening the layout.
