//! Fast scan: the MFT is read by an elevated copy of this program. Windows shows its
//! consent prompt for this program's own executable, and no console window opens.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use std::time::{Duration, Instant};
use timberstat_ntfs::{SummaryProgress, VolumeSummary};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Minimum gap between progress-file writes while reading records.
const PROGRESS_EVERY: Duration = Duration::from_millis(200);

/// Stops the PowerShell launcher from opening a console window.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Files shared between the window and the elevated helper, unique per window run.
#[derive(Debug, Clone)]
pub struct FastPaths {
    /// The helper writes its summary (or an `error=` line) here.
    pub output: PathBuf,
    /// The helper writes the full folder tree here, in `timberstat_ntfs::write_tree` format.
    pub tree: PathBuf,
    /// The helper creates this as soon as it starts, which means approval was given.
    pub started: PathBuf,
    /// The helper rewrites this with its current stage, for the progress bar.
    pub progress: PathBuf,
}

impl FastPaths {
    pub fn for_this_run() -> Self {
        let base = std::env::temp_dir().join(format!("timberstat-fast-{}", std::process::id()));
        Self {
            output: base.with_extension("txt"),
            tree: base.with_extension("tree"),
            started: base.with_extension("started"),
            progress: base.with_extension("progress"),
        }
    }

    pub fn clear(&self) {
        let _ = std::fs::remove_file(&self.output);
        let _ = std::fs::remove_file(&self.tree);
        let _ = std::fs::remove_file(&self.started);
        let _ = std::fs::remove_file(&self.progress);
    }
}

/// Converts a drive path such as `C:\` into the raw device name `//./C:`.
pub fn raw_volume_name(drive: &Path) -> Option<String> {
    let letter = drive.to_string_lossy().chars().next()?;
    letter
        .is_ascii_alphabetic()
        .then(|| format!("//./{}:", letter.to_ascii_uppercase()))
}

/// Runs this program elevated as `--fast-helper <volume> <output>` and reads back its
/// summary and full folder tree. Blocks until the helper exits, so call it off the UI thread.
pub fn run_elevated(
    volume: &str,
    paths: &FastPaths,
) -> Result<(VolumeSummary, timberstat_filesystem::FileTree), String> {
    paths.clear();
    let result = run_elevated_inner(volume, paths);
    paths.clear();
    result
}

fn run_elevated_inner(
    volume: &str,
    paths: &FastPaths,
) -> Result<(VolumeSummary, timberstat_filesystem::FileTree), String> {
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;

    // Start-Process with -Verb RunAs is the only way without unsafe FFI. PowerShell
    // only launches the helper; the consent prompt names the helper executable itself.
    let script = format!(
        "$p = Start-Process -FilePath {exe} -ArgumentList @('--fast-helper', {volume}, {out}) -Verb RunAs -WindowStyle Hidden -Wait -PassThru; exit $p.ExitCode",
        exe = ps_quote(&exe.to_string_lossy()),
        volume = ps_quote(volume),
        out = ps_quote(&paths.output.to_string_lossy()),
    );
    let mut launcher = Command::new("powershell.exe");
    launcher.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-WindowStyle",
        "Hidden",
        "-Command",
        &script,
    ]);
    #[cfg(windows)]
    launcher.creation_flags(CREATE_NO_WINDOW);
    let status = launcher
        .status()
        .map_err(|error| format!("could not start the elevation launcher: {error}"))?;

    let text = std::fs::read_to_string(&paths.output).map_err(|_| {
        if status.success() {
            "the helper finished without writing a result".to_string()
        } else {
            "administrator permission was declined, or the helper could not start".to_string()
        }
    });
    let text = text?;

    if let Some(message) = text.lines().find_map(|line| line.strip_prefix("error=")) {
        return Err(message.to_string());
    }
    let summary = VolumeSummary::from_text(&text)
        .ok_or_else(|| "the helper's result was incomplete".to_string())?;
    let file = std::fs::File::open(&paths.tree)
        .map_err(|error| format!("the helper's folder tree is missing: {error}"))?;
    let tree = timberstat_ntfs::read_tree(std::io::BufReader::with_capacity(1 << 20, file))
        .map_err(|error| format!("could not read the folder tree: {error}"))?;
    Ok((summary, tree))
}

/// Entry point for the elevated helper. `args` are the arguments after `--fast-helper`.
/// Creates the started marker first, then always writes a result file, including on
/// failure, so the window can show why.
pub fn helper_main(args: &[String]) -> i32 {
    let (Some(volume), Some(output)) = (args.first(), args.get(1)) else {
        return 2;
    };
    let output = PathBuf::from(output);
    let progress_path = output.with_extension("progress");
    let tree_path = output.with_extension("tree");
    let _ = std::fs::write(output.with_extension("started"), "");

    let mut last_write: Option<Instant> = None;
    let result = timberstat_ntfs::summarize_volume_with_progress(volume, |stage| {
        // Record reads come thousands of times a second; write the file at most every
        // PROGRESS_EVERY. Stage changes (such as building the tree) are always written.
        let reading = matches!(stage, SummaryProgress::Reading { .. });
        if reading && last_write.is_some_and(|at| at.elapsed() < PROGRESS_EVERY) {
            return;
        }
        last_write = Some(Instant::now());
        let _ = std::fs::write(&progress_path, stage_line(stage));
    });
    let text = match result {
        Ok((summary, tree)) => {
            // The tree goes first: the window only looks at the summary file once it is complete.
            let written = std::fs::File::create(&tree_path)
                .and_then(|file| timberstat_ntfs::write_tree(&tree, file).map(|_| ()));
            match written {
                Ok(()) => summary.to_text(),
                Err(error) => format!("error=could not save the folder tree: {error}\n"),
            }
        }
        Err(error) => format!("error={}\n", error.to_string().replace('\n', " ")),
    };
    match std::fs::write(&output, text) {
        Ok(()) => 0,
        Err(_) => 3,
    }
}

/// One line for the progress file: `done total`, or `build`.
fn stage_line(stage: SummaryProgress) -> String {
    match stage {
        SummaryProgress::Reading { done, total } => format!("{done} {total}\n"),
        SummaryProgress::Building => "build\n".to_string(),
    }
}

/// Reads the helper's latest stage. `None` until the helper has written one.
pub fn read_stage(path: &Path) -> Option<SummaryProgress> {
    let text = std::fs::read_to_string(path).ok()?;
    let line = text.trim();
    if line == "build" {
        return Some(SummaryProgress::Building);
    }
    let mut parts = line.split_whitespace();
    let done = parts.next()?.parse().ok()?;
    let total = parts.next()?.parse().ok()?;
    Some(SummaryProgress::Reading { done, total })
}

/// Quotes a value for PowerShell single-quoted strings.
fn ps_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drive_paths_become_raw_volume_names() {
        assert_eq!(
            raw_volume_name(Path::new("C:\\")).as_deref(),
            Some("//./C:")
        );
        assert_eq!(raw_volume_name(Path::new("w:/")).as_deref(), Some("//./W:"));
        assert_eq!(raw_volume_name(Path::new("/mnt")), None);
    }

    #[test]
    fn powershell_quotes_are_doubled() {
        assert_eq!(ps_quote("it's"), "'it''s'");
    }

    #[test]
    fn started_marker_sits_beside_the_output() {
        let paths = FastPaths::for_this_run();
        assert_eq!(paths.started.with_extension("txt"), paths.output);
    }
}
