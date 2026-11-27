//! Reproducible synthetic dataset generator.
//!
//! Usage:
//!   gen-dataset <small|100k|1m|deep|many-small|few-huge|all> [--root PATH] [--clean]
//!
//! Without `--root` each dataset goes to `<temp>/timberstat-bench-<id>`. `--root`
//! must still resolve inside the temp directory with a `timberstat-bench-` name.
//! `--clean` removes an existing dataset at that root before generating.

use std::{path::PathBuf, process::ExitCode};

use timberstat_perf::datasets::{self, Kind};

fn usage() -> ExitCode {
    eprintln!(
        "usage: gen-dataset <small|100k|1m|deep|many-small|few-huge|all> [--root PATH] [--clean]"
    );
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let mut targets: Vec<String> = Vec::new();
    let mut root: Option<PathBuf> = None;
    let mut clean = false;
    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--root" => match iter.next() {
                Some(path) => root = Some(PathBuf::from(path)),
                None => return usage(),
            },
            "--clean" => clean = true,
            other => targets.push(other.to_owned()),
        }
    }
    if targets.is_empty() {
        return usage();
    }
    let kinds: Vec<Kind> = if targets.iter().any(|t| t == "all") {
        if targets.len() != 1 {
            return usage();
        }
        Kind::ALL.to_vec()
    } else {
        let mut parsed = Vec::new();
        for target in &targets {
            match Kind::parse(target) {
                Some(kind) => parsed.push(kind),
                None => return usage(),
            }
        }
        parsed
    };
    if root.is_some() && kinds.len() != 1 {
        eprintln!("--root can only be used with a single dataset");
        return ExitCode::from(2);
    }
    for kind in kinds {
        let dataset_root = root.clone().unwrap_or_else(|| kind.default_root());
        if clean && let Err(err) = datasets::clean(&dataset_root) {
            eprintln!("error: {err}");
            return ExitCode::FAILURE;
        }
        match datasets::generate(kind, &dataset_root) {
            Ok(manifest) => match serde_json::to_string_pretty(&manifest) {
                Ok(text) => println!("{text}"),
                Err(err) => {
                    eprintln!("error: {err}");
                    return ExitCode::FAILURE;
                }
            },
            Err(err) => {
                eprintln!("error: {err}");
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}
