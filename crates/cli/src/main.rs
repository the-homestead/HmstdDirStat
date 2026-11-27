mod args;
mod cancel;
mod compare_command;
mod dupes_command;
mod engine;
mod error;
mod hash_command;
mod large;
mod output;
mod profile;
mod scan_command;

use std::process::ExitCode;
use std::time::Instant;

use args::Command;
use cancel::Cancellation;
use error::{CliResult, EXIT_USAGE};

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("error: {}", error.message);
            if error.code == EXIT_USAGE {
                eprintln!("try 'timberstat-cli --help'");
            }
            ExitCode::from(error.code)
        }
    }
}

fn run() -> CliResult<u8> {
    let command = args::parse(std::env::args_os().skip(1))?;
    match command {
        Command::Help => {
            output::stdout(args::HELP)?;
            return Ok(error::EXIT_SUCCESS);
        }
        Command::Version => {
            output::stdout(concat!("timberstat-cli ", env!("CARGO_PKG_VERSION"), "\n"))?;
            return Ok(error::EXIT_SUCCESS);
        }
        _ => {}
    }

    let cancellation = Cancellation::install()?;
    // Whole-command timer: starts before the work and is reported after it, success or failure.
    let started = Instant::now();
    let result = match command {
        Command::Scan { root, export } => scan_command::run(&root, &export, &cancellation),
        Command::Large { root, top } => large::run(&root, top, &cancellation),
        Command::Hash { file } => hash_command::run(&file, &cancellation),
        Command::Dupes { root, export } => dupes_command::run(&root, &export, &cancellation),
        Command::Compare { before, after } => compare_command::run(&before, &after, &cancellation),
        Command::Profile { name, root } => profile::run(&name, &root, &cancellation),
        Command::Help | Command::Version => unreachable!("handled before signal setup"),
    };
    eprintln!(
        "{}",
        timberstat_core::timing::line("total", started.elapsed())
    );
    result
}
