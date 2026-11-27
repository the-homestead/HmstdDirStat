# Benchmarks

How TimberStat's performance is measured. Results and hardware are in
[`../BENCHMARKS.md`](../BENCHMARKS.md). Method rules are in
[`../docs/PERFORMANCE_PLAN.md`](../docs/PERFORMANCE_PLAN.md).

## What is here

| Path | Role |
|---|---|
| `crates/perf` (`timberstat-perf`) | Dataset generator (`gen-dataset`), timed scan runner (`perf-scan`), Criterion benches (`benches/scan.rs`). Not shipped. |
| `benchmarks/measure-memory.ps1` | Windows macrobench: runs `perf-scan` as separate processes, records wall time and peak working set, writes an environment record. |
| `benchmarks/results/<UTC stamp>/` | Raw output of each measured session: `environment.json`, `runs-<dataset>.jsonl`, `summary.json`. |
| `.github/workflows/nightly-perf.yml` | Nightly, informational: the ignored 100k scanner test and the Criterion benches. |

## Safety

Datasets are synthetic and live only under the system temp directory
(`%TEMP%`, normally `C:\Users\<user>\AppData\Local\Temp`). `gen-dataset` refuses any
root outside the temp directory and any folder name that does not start with
`timberstat-bench-`. It never writes to a user's drive or project folder.

## Datasets

`cargo run -p timberstat-perf --release --bin gen-dataset -- <id>`

| Id | Shape | Files | Folders | Max depth |
|---|---|---|---|---|
| `small` | flat, 0-4 KiB | 300 | 1 | 1 |
| `100k` | 100 folders x 1000 files, 16 bytes | 100 000 | 100 | 1 |
| `1m` | 1000 folders x 1000 files, 0-4 KiB | 1 000 000 | 1 000 | 1 |
| `deep` | 220 nested folders, 2 files each | 440 | 220 | 220 |
| `many-small` | 20 folders x 1000 files, 0-4 KiB | 20 000 | 20 | 1 |
| `few-huge` | 4 files of 1 GiB logical length | 4 | 1 | 1 |

Each dataset writes `<temp>/timberstat-bench-<id>.manifest.json` with its counts,
size classes and the expected tree node count. `perf-scan` checks the scan against
that count and reports `nodes_match_manifest`. A mismatch fails the measurement.

Size notes: `few-huge` uses `set_len`, so its logical size is 1 GiB each. Whether
NTFS allocates clusters for it depends on the volume; the manifest records logical
bytes only.

Generation is deterministic (fixed seed, `GENERATOR_VERSION` in the manifest).

## Method

1. **Release builds only.** `cargo build -p timberstat-perf --release`. Results from
   debug builds are not recorded.
2. **One process per sample** in the macrobench. Each sample is a fresh process, so
   peak working set is the OS counter for that scan alone.
3. **At least 5 samples** per dataset. Report median and min/max. Never report the
   best run.
4. **Cache state** must be labelled. `measure-memory.ps1` takes `-CacheLabel` and
   records it as given. It does not flush the OS cache. A `cold` label is only valid
   after a flush or reboot that is documented in the run's `environment.json`.
5. **Same-machine comparisons only.** Numbers from another machine are not comparable.
6. **Record the environment** each session: CPU, RAM, OS build, elevation, temp
   volume filesystem, disk media, binary SHA-256, rustc version, repo commit and
   count of uncommitted paths.
7. **Commit raw output.** A number appears in `BENCHMARKS.md` only if it was copied
   from a file under `benchmarks/results/` or from Criterion output that is saved
   with the session.

## Running the macrobench (Windows)

```powershell
cargo build -p timberstat-perf --release --bins
cargo run -p timberstat-perf --release --bin gen-dataset -- small deep many-small
powershell -NoProfile -ExecutionPolicy Bypass -File benchmarks/measure-memory.ps1 `
  -Binary target/release/perf-scan.exe -Datasets small,deep,many-small -Runs 5
```

Generate larger datasets (`100k`, `1m`, `few-huge`) only when the temp volume has room.
Remove them afterwards with `gen-dataset <id> --clean`.

## Running Criterion

```sh
cargo bench -p timberstat-perf
```

Datasets for `scan_generic` are generated on first use under the temp directory.
Criterion results are local and noisy on a laptop; treat them as relative
comparisons within one session only.

## Nightly

`.github/workflows/nightly-perf.yml` runs the ignored scanner test
`hundred_thousand_files_scan` in release mode, and saves its timing line to the job
summary and an artifact. Both jobs are `continue-on-error`. No job compares against a
stored number.

## Not yet measured

The Plan §18 list has items that no benchmark here covers yet: treemap and radial
generation, rendering, search latency, hash throughput, file-operation throughput, UI
responsiveness, and the NTFS MFT scanner against the generic one (the MFT path needs an
elevated session and is not run). `BENCHMARKS.md` lists them as not measured.
