use std::path::Path;

use serde::Serialize;
use timberstat_duplicates::{digest_hex, hash_file};

use crate::{
    cancel::Cancellation,
    error::{CliError, CliResult, EXIT_SUCCESS},
    output,
};

#[derive(Debug, Serialize)]
struct HashDocument {
    algorithm: &'static str,
    path: String,
    bytes: u64,
    digest: String,
}

pub fn run(file: &Path, cancel: &Cancellation) -> CliResult<u8> {
    cancel.check()?;
    let metadata = std::fs::metadata(file).map_err(|error| {
        CliError::failure(format!("could not inspect {}: {error}", file.display()))
    })?;
    if !metadata.is_file() {
        return Err(CliError::failure(format!(
            "{} is not a regular file",
            file.display()
        )));
    }
    eprintln!("hash: BLAKE3 {}", file.display());
    let digest = hash_file(file, cancel.duplicate_progress())
        .map_err(|error| CliError::failure(format!("could not hash {}: {error}", file.display())))?
        .ok_or_else(CliError::cancelled)?;
    let contents = output::json(&HashDocument {
        algorithm: "blake3",
        path: output::unicode_path(file)?,
        bytes: metadata.len(),
        digest: digest_hex(&digest),
    })?;
    cancel.check()?;
    output::stdout(&contents)?;
    eprintln!("hash: complete");
    Ok(EXIT_SUCCESS)
}
