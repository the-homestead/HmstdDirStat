use std::path::Path;
use std::time::Instant;

use timberstat_duplicates::{DuplicateReport, Options, RunStatus, file_items, find_duplicates};
use timberstat_export::{duplicates_to_csv, duplicates_to_json};
use timberstat_fileops::tree_base;

use crate::{
    args::{ExportOptions, Format},
    cancel::Cancellation,
    engine::{self, Scanned},
    error::{CliError, CliResult, EXIT_PARTIAL},
    output,
};

pub fn run(root: &Path, export: &ExportOptions, cancel: &Cancellation) -> CliResult<u8> {
    let started = Instant::now();
    let scanned = engine::scan(root, cancel)?;
    eprintln!(
        "{}",
        timberstat_core::timing::line("scan", started.elapsed())
    );
    let started = Instant::now();
    let report = analyze(&scanned, cancel)?;
    eprintln!(
        "{}",
        timberstat_core::timing::line("duplicates", started.elapsed())
    );
    let mut status = scanned.exit_code();
    if !report.failures.is_empty() {
        status = EXIT_PARTIAL;
    }
    cancel.check()?;
    let contents = match export.format {
        Format::Json => duplicates_to_json(&scanned.tree, &report, &scanned.provenance),
        Format::Csv => duplicates_to_csv(&scanned.tree, &report),
    }
    .map_err(|error| CliError::failure(error.to_string()))?;
    output::emit(&contents, export, cancel)?;
    Ok(status)
}

pub fn analyze(scanned: &Scanned, cancel: &Cancellation) -> CliResult<DuplicateReport> {
    cancel.check()?;
    eprintln!("duplicates: finding byte-verified duplicate files");
    let items = file_items(&scanned.tree, &tree_base(&scanned.root));
    let report = find_duplicates(&items, &Options::default(), cancel.duplicate_progress());
    match &report.status {
        RunStatus::Complete => {}
        RunStatus::Cancelled => return Err(CliError::cancelled()),
        RunStatus::Failed(message) => {
            return Err(CliError::failure(format!(
                "duplicate search failed: {message}"
            )));
        }
    }
    eprintln!(
        "duplicates: {} groups, {} failures",
        report.groups.len(),
        report.failures.len()
    );
    for failure in report.failures.iter().take(5) {
        eprintln!(
            "duplicates: issue {}: {}",
            failure.path.display(),
            failure.detail
        );
    }
    if !report.failures.is_empty() {
        eprintln!("duplicates: result is partial; exit code will be {EXIT_PARTIAL}");
    }
    Ok(report)
}
