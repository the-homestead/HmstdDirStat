use std::path::Path;

use crate::{
    args::ExportOptions,
    cancel::Cancellation,
    dupes_command,
    error::{CliError, CliResult},
    large, scan_command,
};

pub const LARGE_FILES_TOP: usize = 20;

pub fn run(name: &str, root: &Path, cancel: &Cancellation) -> CliResult<u8> {
    eprintln!("profile: running built-in profile '{name}'");
    match name {
        "inventory" => scan_command::run(root, &ExportOptions::json_stdout(), cancel),
        "large-files" => large::run(root, LARGE_FILES_TOP, cancel),
        "duplicates" => dupes_command::run(root, &ExportOptions::json_stdout(), cancel),
        _ => Err(CliError::usage(format!(
            "unknown built-in profile '{name}'; available: inventory, large-files, duplicates"
        ))),
    }
}
