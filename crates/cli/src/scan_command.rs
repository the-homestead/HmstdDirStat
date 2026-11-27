use std::path::Path;

use timberstat_export::{scan_to_csv, scan_to_json};

use crate::{
    args::{ExportOptions, Format},
    cancel::Cancellation,
    engine,
    error::{CliError, CliResult},
    output,
};

pub fn run(root: &Path, export: &ExportOptions, cancel: &Cancellation) -> CliResult<u8> {
    let scanned = engine::scan(root, cancel)?;
    cancel.check()?;
    let contents = match export.format {
        Format::Json => scan_to_json(&scanned.tree, &scanned.provenance),
        Format::Csv => scan_to_csv(&scanned.tree),
    }
    .map_err(|error| CliError::failure(error.to_string()))?;
    let status = scanned.exit_code();
    output::emit(&contents, export, cancel)?;
    Ok(status)
}
