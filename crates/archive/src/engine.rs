//! Public manager: detection + execution + parsing wired together.
//!
//! Nothing here uses shell strings: [`crate::runner::run`] receives the argv
//! produced by [`crate::ops::build`], every time.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::{ArchiveError, ArchiveResult};
use crate::format::ArchiveFormat;
use crate::handler::HandlerCatalog;
use crate::ops::{Op, build};
use crate::parse::{parse_7z, parse_stream_listing, parse_tar_verbose, parse_unzip};
use crate::runner::run;

/// A parsed archive listing produced by a handler.
pub struct ArchiveListing {
    pub format: ArchiveFormat,
    pub path: PathBuf,
    pub entries: Vec<crate::parse::ParsedEntry>,
    pub total_original: Option<u64>,
    pub total_compressed: Option<u64>,
}

pub struct ArchiveManager {
    catalog: HandlerCatalog,
}

impl Default for ArchiveManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ArchiveManager {
    /// Detect all handlers present on this host.
    pub fn new() -> Self {
        Self {
            catalog: HandlerCatalog::detect(),
        }
    }

    pub fn catalog(&self) -> &HandlerCatalog {
        &self.catalog
    }

    /// Formats that lack any usable handler on this host.
    pub fn missing_summary(&self) -> Option<String> {
        self.catalog.missing_summary()
    }

    pub fn missing_message(&self, format: ArchiveFormat) -> Option<String> {
        self.catalog.missing_message(format)
    }

    fn read_handler(&self, format: ArchiveFormat) -> ArchiveResult<&crate::handler::HandlerSpec> {
        self.catalog.read_handler(format).ok_or_else(|| {
            ArchiveError::NoHandler(self.catalog.missing_message(format).unwrap_or_default())
        })
    }

    pub fn list(&self, archive: &Path) -> ArchiveResult<ArchiveListing> {
        let format = ArchiveFormat::from_path(archive)
            .ok_or_else(|| ArchiveError::UnknownFormat(archive.to_path_buf()))?;
        let handler = self.read_handler(format)?;
        let spec = build(handler, format, Op::List, archive, None, None, None)?;
        let outcome = run(
            &spec.program,
            &spec.args,
            None,
            Duration::from_secs(60),
            None,
        )?;
        if outcome.status != Some(0) {
            return Err(ArchiveError::handler_failed(
                handler.name.clone(),
                outcome.status,
                outcome.stderr,
            ));
        }
        let parsed = match format {
            ArchiveFormat::SevenZip | ArchiveFormat::Zip => {
                if handler.family == crate::handler::HandlerFamily::SevenZip {
                    parse_7z(&outcome.stdout)
                } else {
                    parse_unzip(&outcome.stdout)
                }
            }
            ArchiveFormat::Tar
            | ArchiveFormat::TarGz
            | ArchiveFormat::TarXz
            | ArchiveFormat::TarZst => parse_tar_verbose(&outcome.stdout),
            ArchiveFormat::Gzip
            | ArchiveFormat::Bzip2
            | ArchiveFormat::Xz
            | ArchiveFormat::Zstandard => parse_stream_listing(&outcome.stdout),
            _ => {
                return Err(ArchiveError::Unsupported(format.label().to_owned()));
            }
        };
        Ok(ArchiveListing {
            format,
            path: archive.to_path_buf(),
            entries: parsed.entries,
            total_original: parsed.total_original,
            total_compressed: parsed.total_compressed,
        })
    }

    pub fn test(&self, archive: &Path) -> ArchiveResult<()> {
        let format = ArchiveFormat::from_path(archive)
            .ok_or_else(|| ArchiveError::UnknownFormat(archive.to_path_buf()))?;
        let handler = self.read_handler(format)?;
        match format {
            ArchiveFormat::Tar
            | ArchiveFormat::TarGz
            | ArchiveFormat::TarXz
            | ArchiveFormat::TarZst => {
                // tar has no test mode; extract to a scratch directory and drop it.
                let temp: PathBuf = std::env::temp_dir().join("hmstd-archive-test");
                let spec = build(
                    handler,
                    format,
                    Op::Extract,
                    archive,
                    Some(&temp),
                    None,
                    None,
                )?;
                let outcome = run(
                    &spec.program,
                    &spec.args,
                    None,
                    Duration::from_secs(120),
                    None,
                )?;
                if outcome.status != Some(0) {
                    return Err(ArchiveError::handler_failed(
                        handler.name.clone(),
                        outcome.status,
                        outcome.stderr,
                    ));
                }
                Ok(())
            }
            _ => {
                let spec = build(handler, format, Op::Test, archive, None, None, None)?;
                let outcome = run(
                    &spec.program,
                    &spec.args,
                    None,
                    Duration::from_secs(120),
                    None,
                )?;
                if outcome.status != Some(0) {
                    return Err(ArchiveError::handler_failed(
                        handler.name.clone(),
                        outcome.status,
                        outcome.stderr,
                    ));
                }
                Ok(())
            }
        }
    }

    pub fn extract(&self, archive: &Path, dest_dir: &Path) -> ArchiveResult<ArchiveListing> {
        std::fs::create_dir_all(dest_dir).map_err(|e| ArchiveError::Io {
            path: dest_dir.to_path_buf(),
            detail: e.to_string(),
        })?;
        let format = ArchiveFormat::from_path(archive)
            .ok_or_else(|| ArchiveError::UnknownFormat(archive.to_path_buf()))?;
        let handler = self.read_handler(format)?;
        let spec = build(
            handler,
            format,
            Op::Extract,
            archive,
            Some(dest_dir),
            None,
            None,
        )?;
        let outcome = run(
            &spec.program,
            &spec.args,
            None,
            Duration::from_secs(300),
            None,
        )?;
        if outcome.status != Some(0) {
            return Err(ArchiveError::handler_failed(
                handler.name.clone(),
                outcome.status,
                outcome.stderr,
            ));
        }
        self.list(archive)
    }

    pub fn create(&self, archive: &Path, files: &[PathBuf]) -> ArchiveResult<()> {
        let format = ArchiveFormat::from_path(archive)
            .ok_or_else(|| ArchiveError::UnknownFormat(archive.to_path_buf()))?;
        let handler = self.catalog.create_handler(format).ok_or_else(|| {
            ArchiveError::NoHandler(self.catalog.missing_message(format).unwrap_or_default())
        })?;
        let spec = build(
            handler,
            format,
            Op::Create,
            archive,
            None,
            None,
            Some(files),
        )?;
        let outcome = run(
            &spec.program,
            &spec.args,
            None,
            Duration::from_secs(300),
            None,
        )?;
        if outcome.status != Some(0) {
            return Err(ArchiveError::handler_failed(
                handler.name.clone(),
                outcome.status,
                outcome.stderr,
            ));
        }
        Ok(())
    }

    pub fn add(&self, archive: &Path, files: &[PathBuf]) -> ArchiveResult<()> {
        let format = ArchiveFormat::from_path(archive)
            .ok_or_else(|| ArchiveError::UnknownFormat(archive.to_path_buf()))?;
        let handler = self.catalog.create_handler(format).ok_or_else(|| {
            ArchiveError::NoHandler(self.catalog.missing_message(format).unwrap_or_default())
        })?;
        let spec = build(handler, format, Op::Add, archive, None, None, Some(files))?;
        let outcome = run(
            &spec.program,
            &spec.args,
            None,
            Duration::from_secs(300),
            None,
        )?;
        if outcome.status != Some(0) {
            return Err(ArchiveError::handler_failed(
                handler.name.clone(),
                outcome.status,
                outcome.stderr,
            ));
        }
        Ok(())
    }
}
