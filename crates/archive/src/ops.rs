//! argv builders for each handler family and operation.
//!
//! Nothing here runs a process: the engine calls [`crate::runner::run`] with
//! the argv these functions return. A format the family cannot serve yields
//! [`OpError::Unsupported`], which the engine reports instead of faking success.

use std::path::{Path, PathBuf};

use crate::error::ArchiveResult;
use crate::format::ArchiveFormat;
use crate::handler::{HandlerFamily, HandlerSpec};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    List,
    Test,
    Extract,
    Create,
    Add,
}

/// A ready-to-run handler invocation.
#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
}

/// Why an operation cannot be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpError {
    pub reason: String,
}

impl OpError {
    fn unsupported(format: ArchiveFormat, handler: &HandlerSpec, what: &str) -> Self {
        Self {
            reason: format!(
                "{} cannot {} a {} archive",
                handler.name,
                what,
                format.label()
            ),
        }
    }
}

fn sevenzip_type(format: ArchiveFormat) -> Option<&'static str> {
    match format {
        ArchiveFormat::Zip => Some("zip"),
        ArchiveFormat::SevenZip => Some("7z"),
        ArchiveFormat::Tar => Some("tar"),
        ArchiveFormat::Gzip => Some("gzip"),
        ArchiveFormat::Bzip2 => Some("bzip2"),
        ArchiveFormat::Xz => Some("xz"),
        ArchiveFormat::Zstandard => Some("zstd"),
        _ => None,
    }
}

fn single_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

impl From<OpError> for crate::error::ArchiveError {
    fn from(error: OpError) -> Self {
        crate::error::ArchiveError::Unsupported(error.reason)
    }
}

/// Build the argv for `op` on `archive`.
pub fn build(
    handler: &HandlerSpec,
    format: ArchiveFormat,
    op: Op,
    archive: &Path,
    dest_dir: Option<&Path>,
    selection: Option<&[String]>,
    files: Option<&[PathBuf]>,
) -> ArchiveResult<CommandSpec> {
    let program = handler.program.clone();
    let archive_arg = single_string(archive);

    let args = match (handler.family, op) {
        // ------------------------------------------------------------------
        // 7-Zip family (7z, NanaZip console)
        // ------------------------------------------------------------------
        (HandlerFamily::SevenZip, Op::List) => vec!["l".into(), "-slt".into(), archive_arg],
        (HandlerFamily::SevenZip, Op::Test) => vec!["t".into(), archive_arg],
        (HandlerFamily::SevenZip, Op::Extract) => {
            let dest = dest_dir.ok_or(OpError {
                reason: "7-Zip extraction needs a destination directory".into(),
            })?;
            let mut argv = vec![
                "x".to_owned(),
                "-y".to_owned(),
                format!("-o{}", single_string(dest)),
                archive_arg,
            ];
            if let Some(selection) = selection {
                argv.extend(selection.iter().cloned());
            }
            argv
        }
        (HandlerFamily::SevenZip, Op::Create) => {
            let kind = sevenzip_type(format)
                .ok_or_else(|| OpError::unsupported(format, handler, "create"))?;
            let mut argv = vec![
                "a".to_owned(),
                "-y".to_owned(),
                format!("-t{kind}"),
                archive_arg,
            ];
            if let Some(files) = files {
                argv.extend(files.iter().map(|p| single_string(p)));
            }
            argv
        }
        (HandlerFamily::SevenZip, Op::Add) => {
            let mut argv = vec!["a".to_owned(), "-y".to_owned(), archive_arg];
            if let Some(files) = files {
                argv.extend(files.iter().map(|p| single_string(p)));
            }
            argv
        }

        // ------------------------------------------------------------------
        // tar / bsdtar family
        // ------------------------------------------------------------------
        (HandlerFamily::Tar, Op::List) => vec!["-tvf".into(), archive_arg],
        (HandlerFamily::Tar, Op::Extract) => {
            let dest = dest_dir.ok_or(OpError {
                reason: "tar extraction needs a destination directory".into(),
            })?;
            let mut argv = vec![
                "-xf".to_owned(),
                archive_arg,
                "-C".to_owned(),
                single_string(dest),
            ];
            if let Some(selection) = selection {
                argv.extend(selection.iter().cloned());
            }
            argv
        }
        (HandlerFamily::Tar, Op::Create) => {
            let compress = match format {
                ArchiveFormat::Tar => "",
                ArchiveFormat::TarGz => "-a",
                ArchiveFormat::TarXz => "-a",
                ArchiveFormat::TarZst => "-a",
                _ => {
                    return Err(OpError::unsupported(format, handler, "create").into());
                }
            };
            let mut argv: Vec<String> = Vec::new();
            if !compress.is_empty() {
                argv.push(compress.to_owned());
            }
            argv.push("-cf".to_owned());
            argv.push(archive_arg);
            if let Some(files) = files {
                argv.extend(files.iter().map(|p| single_string(p)));
            }
            argv
        }
        (HandlerFamily::Tar, Op::Add) => {
            let mut argv = vec!["-rf".to_owned(), archive_arg];
            if let Some(files) = files {
                argv.extend(files.iter().map(|p| single_string(p)));
            }
            argv
        }
        (HandlerFamily::Tar, Op::Test) => {
            return Err(OpError {
                reason:
                    "tar has no test mode; the engine tests by extracting to a temporary directory"
                        .into(),
            }
            .into());
        }

        // ------------------------------------------------------------------
        // unzip
        // ------------------------------------------------------------------
        (HandlerFamily::Unzip, Op::List) => vec!["-l".into(), archive_arg],
        (HandlerFamily::Unzip, Op::Test) => vec!["-t".into(), archive_arg],
        (HandlerFamily::Unzip, Op::Extract) => {
            let dest = dest_dir.ok_or(OpError {
                reason: "unzip needs a destination directory".into(),
            })?;
            let mut argv = vec![
                "-n".to_owned(),
                "-d".to_owned(),
                single_string(dest),
                archive_arg,
            ];
            if let Some(selection) = selection {
                argv.extend(selection.iter().cloned());
            }
            argv
        }
        (HandlerFamily::Unzip, Op::Create) | (HandlerFamily::Unzip, Op::Add) => {
            return Err(OpError::unsupported(format, handler, "create").into());
        }

        // ------------------------------------------------------------------
        // zip CLI
        // ------------------------------------------------------------------
        (HandlerFamily::Zip, Op::Create) => {
            let mut argv = vec!["-r".to_owned(), archive_arg];
            if let Some(files) = files {
                argv.extend(files.iter().map(|p| single_string(p)));
            }
            argv
        }
        (HandlerFamily::Zip, Op::Add) => {
            let mut argv = vec![archive_arg];
            if let Some(files) = files {
                argv.extend(files.iter().map(|p| single_string(p)));
            }
            argv
        }
        (HandlerFamily::Zip, Op::List)
        | (HandlerFamily::Zip, Op::Test)
        | (HandlerFamily::Zip, Op::Extract) => {
            return Err(OpError::unsupported(format, handler, "read").into());
        }

        // ------------------------------------------------------------------
        // Single-stream compressors (gzip, bzip2, xz, zstd)
        // ------------------------------------------------------------------
        (family, Op::List) => match family {
            HandlerFamily::Gzip => vec!["-l".into(), archive_arg],
            HandlerFamily::Xz => vec!["-l".into(), archive_arg],
            HandlerFamily::Zstd => vec!["-l".into(), archive_arg],
            HandlerFamily::Bzip2 => {
                return Err(OpError {
                    reason: "bzip2 has no listing mode; only the compressed size is known".into(),
                }
                .into());
            }
            _ => return Err(OpError::unsupported(format, handler, "list").into()),
        },
        (family, Op::Test) => match family {
            HandlerFamily::Gzip
            | HandlerFamily::Bzip2
            | HandlerFamily::Xz
            | HandlerFamily::Zstd => {
                vec!["-t".into(), archive_arg]
            }
            _ => return Err(OpError::unsupported(format, handler, "test").into()),
        },
        (family, Op::Extract) => match family {
            HandlerFamily::Gzip
            | HandlerFamily::Bzip2
            | HandlerFamily::Xz
            | HandlerFamily::Zstd => {
                vec!["-d".into(), "-c".into(), archive_arg]
            }
            _ => return Err(OpError::unsupported(format, handler, "extract").into()),
        },
        (family, Op::Create) => match family {
            HandlerFamily::Gzip
            | HandlerFamily::Bzip2
            | HandlerFamily::Xz
            | HandlerFamily::Zstd => {
                let Some(files) = files else {
                    return Err(OpError {
                        reason: "compressing needs exactly one input file".into(),
                    }
                    .into());
                };
                let Some(first) = files.first() else {
                    return Err(OpError {
                        reason: "compressing needs exactly one input file".into(),
                    }
                    .into());
                };
                vec!["-c".into(), single_string(first)]
            }
            _ => return Err(OpError::unsupported(format, handler, "create").into()),
        },
        (family, Op::Add) => match family {
            HandlerFamily::Gzip
            | HandlerFamily::Bzip2
            | HandlerFamily::Xz
            | HandlerFamily::Zstd => {
                return Err(OpError {
                    reason: "single-stream compressors cannot add files".into(),
                }
                .into());
            }
            _ => return Err(OpError::unsupported(format, handler, "add").into()),
        },
    };

    Ok(CommandSpec { program, args })
}

/// Format that this handler can create, if any.
pub fn create_type(format: ArchiveFormat) -> Option<&'static str> {
    sevenzip_type(format)
}
