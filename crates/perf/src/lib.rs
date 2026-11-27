//! Benchmark support for TimberStat: reproducible synthetic datasets, a timed
//! scan harness, memory sampling, and the record and summary formats. Not part
//! of the shipped application.
//!
//! Numbers are never written here. Measured results live in `BENCHMARKS.md`,
//! produced from raw output files under `benchmarks/results/`.

pub mod datasets;
pub mod env;
pub mod mem;
pub mod run;
pub mod summary;
