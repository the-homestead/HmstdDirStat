use std::{
    io::{self, Write},
    path::Path,
};

use serde::Serialize;
use timberstat_export::{ExportError, write_atomic_with_check};

use crate::{
    args::ExportOptions,
    cancel::Cancellation,
    error::{CliError, CliResult},
};

pub fn emit(contents: &str, options: &ExportOptions, cancel: &Cancellation) -> CliResult<()> {
    cancel.check()?;
    if let Some(path) = &options.out {
        write_atomic_with_check(path, contents, options.overwrite, &|| {
            if cancel.is_cancelled() {
                Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"))
            } else {
                Ok(())
            }
        })
        .map_err(|error| {
            if cancel.is_cancelled() {
                return CliError::cancelled();
            }
            match error {
                ExportError::AlreadyExists(path) => CliError::failure(format!(
                    "output {} already exists; use --overwrite to replace it",
                    path.display()
                )),
                other => CliError::failure(other.to_string()),
            }
        })?;
        eprintln!("output: wrote {}", path.display());
        Ok(())
    } else {
        stdout(contents)
    }
}

pub fn stdout(contents: &str) -> CliResult<()> {
    let mut output = std::io::stdout().lock();
    output
        .write_all(contents.as_bytes())
        .and_then(|()| output.flush())
        .map_err(|error| CliError::failure(format!("could not write stdout: {error}")))
}

pub fn json(value: &impl Serialize) -> CliResult<String> {
    let mut encoded = serde_json::to_string_pretty(value)
        .map_err(|error| CliError::failure(format!("could not encode JSON: {error}")))?;
    encoded.push('\n');
    Ok(encoded)
}

pub fn unicode_path(path: &Path) -> CliResult<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| CliError::failure(format!("path is not valid Unicode: {path:?}")))
}
