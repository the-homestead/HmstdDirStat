use std::{ffi::OsString, path::PathBuf};

use crate::error::{CliError, CliResult};

pub const HELP: &str = "TimberStat headless CLI\n\n\
Usage:\n\
  timberstat-cli scan <path> [--format json|csv] [--out <file>] [--overwrite]\n\
  timberstat-cli large <path> --top <N>\n\
  timberstat-cli hash <file> [--algorithm blake3]\n\
  timberstat-cli dupes <path> [--format json|csv] [--out <file>] [--overwrite]\n\
  timberstat-cli compare <scanA.json|scanA.tscan> <scanB.json|scanB.tscan>\n\
  timberstat-cli profile run <inventory|large-files|duplicates> <path>\n\n\
Only BLAKE3 hashing is implemented. Other hash algorithms report not implemented.\n\
Progress and diagnostics go to stderr; data goes to stdout or --out.\n\
Exit codes: 0 success, 1 failure, 2 usage, 3 partial result, 130 cancelled.\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Json,
    Csv,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportOptions {
    pub format: Format,
    pub out: Option<PathBuf>,
    pub overwrite: bool,
}

impl ExportOptions {
    pub fn json_stdout() -> Self {
        Self {
            format: Format::Json,
            out: None,
            overwrite: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Scan {
        root: PathBuf,
        export: ExportOptions,
    },
    Large {
        root: PathBuf,
        top: usize,
    },
    Hash {
        file: PathBuf,
    },
    Dupes {
        root: PathBuf,
        export: ExportOptions,
    },
    Compare {
        before: PathBuf,
        after: PathBuf,
    },
    Profile {
        name: String,
        root: PathBuf,
    },
    Help,
    Version,
}

pub fn parse(args: impl IntoIterator<Item = OsString>) -> CliResult<Command> {
    let args: Vec<OsString> = args.into_iter().collect();
    let Some(command) = args.first() else {
        return Err(CliError::usage("missing command"));
    };
    let rest = &args[1..];
    match command.to_str() {
        Some("--help" | "-h" | "help") if rest.is_empty() => Ok(Command::Help),
        Some("--version" | "-V") if rest.is_empty() => Ok(Command::Version),
        Some("scan") => parse_export_command(rest, true),
        Some("large") => parse_large(rest),
        Some("hash") => parse_hash(rest),
        Some("dupes") => parse_export_command(rest, false),
        Some("compare") => parse_compare(rest),
        Some("profile") => parse_profile(rest),
        Some(other) => Err(CliError::usage(format!("unknown command '{other}'"))),
        None => Err(CliError::usage("command must be valid Unicode")),
    }
}

fn parse_export_command(args: &[OsString], scan: bool) -> CliResult<Command> {
    if help_only(args) {
        return Ok(Command::Help);
    }
    let (root, export) = parse_path_and_export(args)?;
    if scan {
        Ok(Command::Scan { root, export })
    } else {
        Ok(Command::Dupes { root, export })
    }
}

fn parse_path_and_export(args: &[OsString]) -> CliResult<(PathBuf, ExportOptions)> {
    let mut root = None;
    let mut format = Format::Json;
    let mut out = None;
    let mut overwrite = false;
    let mut index = 0;
    let mut options = true;
    while index < args.len() {
        let arg = &args[index];
        if options && arg == "--" {
            options = false;
        } else if options && arg == "--format" {
            let value = take_value(args, &mut index, "--format")?;
            format = match value.to_str() {
                Some("json") => Format::Json,
                Some("csv") => Format::Csv,
                _ => return Err(CliError::usage("--format must be json or csv")),
            };
        } else if options && arg == "--out" {
            out = Some(PathBuf::from(take_value(args, &mut index, "--out")?));
        } else if options && arg == "--overwrite" {
            overwrite = true;
        } else if options && arg.to_string_lossy().starts_with('-') {
            return Err(CliError::usage(format!(
                "unknown option '{}'",
                arg.to_string_lossy()
            )));
        } else if root.replace(PathBuf::from(arg)).is_some() {
            return Err(CliError::usage("only one input path is accepted"));
        }
        index += 1;
    }
    let root = root.ok_or_else(|| CliError::usage("missing input path"))?;
    if overwrite && out.is_none() {
        return Err(CliError::usage("--overwrite requires --out"));
    }
    Ok((
        root,
        ExportOptions {
            format,
            out,
            overwrite,
        },
    ))
}

fn parse_large(args: &[OsString]) -> CliResult<Command> {
    if help_only(args) {
        return Ok(Command::Help);
    }
    let mut root = None;
    let mut top = None;
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--top" {
            let value = take_value(args, &mut index, "--top")?;
            let value = unicode(value, "--top")?;
            top = Some(
                value
                    .parse::<usize>()
                    .map_err(|_| CliError::usage("--top must be a positive integer"))?,
            );
        } else if arg.to_string_lossy().starts_with('-') {
            return Err(CliError::usage(format!(
                "unknown option '{}'",
                arg.to_string_lossy()
            )));
        } else if root.replace(PathBuf::from(arg)).is_some() {
            return Err(CliError::usage("large accepts exactly one path"));
        }
        index += 1;
    }
    let root = root.ok_or_else(|| CliError::usage("large requires a path"))?;
    let top = top.ok_or_else(|| CliError::usage("large requires --top N"))?;
    if top == 0 {
        return Err(CliError::usage("--top must be greater than zero"));
    }
    Ok(Command::Large { root, top })
}

fn parse_hash(args: &[OsString]) -> CliResult<Command> {
    if help_only(args) {
        return Ok(Command::Help);
    }
    let mut file = None;
    let mut algorithm = "blake3".to_owned();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--algorithm" || arg == "--algo" {
            algorithm = unicode(take_value(args, &mut index, "--algorithm")?, "--algorithm")?;
        } else if arg.to_string_lossy().starts_with('-') {
            return Err(CliError::usage(format!(
                "unknown option '{}'",
                arg.to_string_lossy()
            )));
        } else if file.replace(PathBuf::from(arg)).is_some() {
            return Err(CliError::usage("hash accepts exactly one file"));
        }
        index += 1;
    }
    if !algorithm.eq_ignore_ascii_case("blake3") {
        return Err(CliError::usage(format!(
            "hash algorithm '{algorithm}' is not implemented; only BLAKE3 is done"
        )));
    }
    Ok(Command::Hash {
        file: file.ok_or_else(|| CliError::usage("hash requires a file"))?,
    })
}

fn parse_compare(args: &[OsString]) -> CliResult<Command> {
    if help_only(args) {
        return Ok(Command::Help);
    }
    if args.len() != 2 {
        return Err(CliError::usage(
            "compare requires exactly two .json or .tscan files",
        ));
    }
    Ok(Command::Compare {
        before: PathBuf::from(&args[0]),
        after: PathBuf::from(&args[1]),
    })
}

fn parse_profile(args: &[OsString]) -> CliResult<Command> {
    if help_only(args) {
        return Ok(Command::Help);
    }
    if args.len() != 3 || args[0] != "run" {
        return Err(CliError::usage(
            "usage: profile run <inventory|large-files|duplicates> <path>",
        ));
    }
    Ok(Command::Profile {
        name: unicode(args[1].clone(), "profile name")?,
        root: PathBuf::from(&args[2]),
    })
}

fn take_value(args: &[OsString], index: &mut usize, flag: &str) -> CliResult<OsString> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| CliError::usage(format!("{flag} requires a value")))
}

fn unicode(value: OsString, name: &str) -> CliResult<String> {
    value
        .into_string()
        .map_err(|_| CliError::usage(format!("{name} must be valid Unicode")))
}

fn help_only(args: &[OsString]) -> bool {
    args.len() == 1 && (args[0] == "--help" || args[0] == "-h")
}
