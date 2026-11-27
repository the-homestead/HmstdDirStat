//! Timed generic-scanner runs on one directory. Prints one JSON object per run.
//!
//! Usage:
//!   perf-scan --root PATH [--runs N] [--cache cold|warm] [--threads N]
//!   perf-scan --env                      print the machine record as JSON
//!   perf-scan summarize FILE...          summarise JSON-lines run files
//!
//! `--cache` is recorded as given. This tool cannot flush the OS cache, so a
//! `cold` label is only valid when the caller flushed it first (benchmarks/README.md).
//! Each invocation is one process; run it once per sample so that the peak working
//! set belongs to a single scan.

use std::{path::PathBuf, process::ExitCode};

use timberstat_perf::{datasets, env, run, summary};
use timberstat_scanner::ScanOptions;

fn usage() -> ExitCode {
    eprintln!(
        "usage: perf-scan (--root PATH | --dataset ID) [--runs N] [--cache cold|warm] [--threads N] [--no-memory]\n       perf-scan --env\n       perf-scan summarize FILE..."
    );
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--env") if args.len() == 1 => print_json(&env::collect(&std::env::temp_dir())),
        Some("summarize") if args.len() > 1 => summarize(&args[1..]),
        _ => scan(&args),
    }
}

fn print_json<T: serde::Serialize>(value: &T) -> ExitCode {
    match serde_json::to_string_pretty(value) {
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn summarize(files: &[String]) -> ExitCode {
    let mut records = Vec::new();
    for file in files {
        let text = match std::fs::read_to_string(file) {
            Ok(text) => text,
            Err(err) => {
                eprintln!("error: {file}: {err}");
                return ExitCode::FAILURE;
            }
        };
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            match serde_json::from_str::<run::RunRecord>(line) {
                Ok(record) => records.push(record),
                Err(err) => {
                    eprintln!("error: {file}: bad record: {err}");
                    return ExitCode::FAILURE;
                }
            }
        }
    }
    print_json(&summary::summarize(&records))
}

fn scan(args: &[String]) -> ExitCode {
    let mut root: Option<PathBuf> = None;
    let mut dataset: Option<String> = None;
    let mut runs: u32 = 1;
    let mut cache = "warm".to_owned();
    let mut threads: usize = 0;
    let mut sample_memory = true;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--no-memory" {
            sample_memory = false;
            continue;
        }
        let value = iter.next();
        match (arg.as_str(), value) {
            ("--root", Some(v)) => root = Some(PathBuf::from(v)),
            ("--dataset", Some(v)) => dataset = Some(v.clone()),
            ("--runs", Some(v)) => match v.parse() {
                Ok(n) if n >= 1 => runs = n,
                _ => return usage(),
            },
            ("--cache", Some(v)) if v == "cold" || v == "warm" => cache = v.clone(),
            ("--threads", Some(v)) => match v.parse() {
                Ok(n) => threads = n,
                Err(_) => return usage(),
            },
            _ => return usage(),
        }
    }
    let Some(root) = root.or_else(|| {
        dataset
            .as_deref()
            .and_then(datasets::Kind::parse)
            .map(datasets::Kind::default_root)
    }) else {
        return usage();
    };
    if !root.is_dir() {
        eprintln!("error: {} is not a directory", root.display());
        return ExitCode::FAILURE;
    }
    let manifest = datasets::read_manifest(&root);
    let options = ScanOptions {
        threads,
        ..ScanOptions::default()
    };
    for n in 1..=runs {
        let plan = run::RunPlan {
            root: &root,
            options: &options,
            cache_label: &cache,
            run: n,
            manifest: manifest.as_ref(),
            sample_memory,
        };
        let record = match run::execute(&plan) {
            Ok(record) => record,
            Err(err) => {
                eprintln!("error: scan failed: {err}");
                return ExitCode::FAILURE;
            }
        };
        match serde_json::to_string(&record) {
            Ok(line) => println!("{line}"),
            Err(err) => {
                eprintln!("error: {err}");
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}
