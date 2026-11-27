//! Fast scan: the NTFS file table is read by one elevated copy of this program, started once per
//! app run. Scans send it requests through a folder for this run; the helper answers in the same
//! folder. Windows shows its consent prompt once, when the helper starts.
//!
//! The helper accepts only drive volumes (`//./X:`), and writes only inside its own folder.

use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use timberstat_diskprobe::elevate::{Elevated, LaunchError};
use timberstat_filesystem::FileTree;
use timberstat_ntfs::{SummaryProgress, VolumeSummary};

/// Minimum gap between progress-file writes while reading records.
const PROGRESS_EVERY: Duration = Duration::from_millis(200);
/// How long the consent prompt may stay open before a request gives up.
const CONSENT_WAIT: Duration = Duration::from_secs(180);
/// The window refreshes its heartbeat this often. The helper exits when the heartbeat goes stale.
const HEARTBEAT_EVERY: Duration = Duration::from_secs(5);
/// Without a fresh heartbeat for this long, the helper assumes the window is gone and exits.
const HELPER_IDLE_LIMIT: Duration = Duration::from_secs(30);
/// How often the helper checks its folder for a request, and the window checks for an answer.
const POLL: Duration = Duration::from_millis(100);

/// Separates the summary from the folder tree inside the one result file.
const TREE_MARKER: &str = "--tree--\n";

/// The folder shared with the helper, unique to this app run.
static SESSION_DIR: OnceLock<PathBuf> = OnceLock::new();
/// Gives each scan its own result file names.
static NEXT_RESULT: AtomicU64 = AtomicU64::new(1);
/// The running helper, if one was started and has not exited.
static HELPER: Mutex<Option<Helper>> = Mutex::new(None);
/// The heartbeat thread starts once per app run.
static HEARTBEAT: OnceLock<()> = OnceLock::new();

struct Helper {
    process: Elevated,
}

/// Files shared between the window and the elevated helper for one scan.
#[derive(Debug, Clone)]
pub struct FastPaths {
    /// The helper writes one file: the summary, a `--tree--` line, then the folder tree.
    /// On failure it writes a single `error=` line instead.
    pub output: PathBuf,
    /// The helper creates this as soon as it starts the scan.
    pub started: PathBuf,
    /// The helper rewrites this with its current stage, for the progress bar.
    pub progress: PathBuf,
}

impl FastPaths {
    pub fn for_this_run() -> Self {
        let seq = NEXT_RESULT.fetch_add(1, Ordering::Relaxed);
        let base = session_dir().join(format!("result-{seq}"));
        Self {
            output: base.with_extension("txt"),
            started: base.with_extension("started"),
            progress: base.with_extension("progress"),
        }
    }

    pub fn clear(&self) {
        let _ = std::fs::remove_file(&self.output);
        let _ = std::fs::remove_file(&self.started);
        let _ = std::fs::remove_file(&self.progress);
    }
}

/// This app run's shared folder: `%LOCALAPPDATA%\TimberStat\fast\<pid>-<time>`.
fn session_dir() -> &'static Path {
    SESSION_DIR.get_or_init(|| {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|time| time.as_nanos())
            .unwrap_or(0);
        base.join("TimberStat")
            .join("fast")
            .join(format!("{}-{nanos}", std::process::id()))
    })
}

/// Converts a drive path such as `C:\` into the raw device name `//./C:`.
pub fn raw_volume_name(drive: &Path) -> Option<String> {
    let letter = drive.to_string_lossy().chars().next()?;
    letter
        .is_ascii_alphabetic()
        .then(|| format!("//./{}:", letter.to_ascii_uppercase()))
}

/// Reads a drive's file table through the elevated helper and returns the summary and the
/// folder tree. The first call starts the helper, which shows one consent prompt. Blocks until
/// the answer arrives, so call it off the UI thread.
pub fn run_elevated(volume: &str, paths: &FastPaths) -> Result<(VolumeSummary, FileTree), String> {
    paths.clear();
    let result = request_scan(volume, paths);
    paths.clear();
    result
}

fn request_scan(volume: &str, paths: &FastPaths) -> Result<(VolumeSummary, FileTree), String> {
    ensure_helper()?;
    let dir = session_dir();
    let temp = dir.join("request.tmp");
    std::fs::write(&temp, format!("{volume}\n{}\n", paths.output.display()))
        .and_then(|()| std::fs::rename(&temp, dir.join("request")))
        .map_err(|error| format!("could not send the scan request: {error}"))?;
    wait_for_output(paths)?;
    read_result(paths)
}

/// Starts the helper if it is not running. Only one helper runs per app run.
fn ensure_helper() -> Result<(), String> {
    let mut guard = HELPER
        .lock()
        .map_err(|_| "the helper state is unavailable".to_string())?;
    if let Some(helper) = guard.as_ref()
        && !helper.process.has_exited()
    {
        return Ok(());
    }
    let dir = session_dir();
    std::fs::create_dir_all(dir)
        .map_err(|error| format!("could not create the helper folder: {error}"))?;
    let _ = std::fs::remove_file(dir.join("request"));
    let _ = std::fs::remove_file(dir.join("ready"));
    start_heartbeat(dir);

    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let arguments = format!("--fast-service \"{}\"", dir.display());
    let process =
        timberstat_diskprobe::elevate::launch(&exe, &arguments).map_err(|error| match error {
            LaunchError::Declined => "administrator permission was declined".to_string(),
            LaunchError::Failed(code) => {
                format!("could not start the elevated helper (Windows error {code})")
            }
        })?;
    let started = Instant::now();
    while !dir.join("ready").exists() {
        if process.has_exited() {
            return Err("the elevated helper exited before it was ready".to_string());
        }
        if started.elapsed() > CONSENT_WAIT {
            return Err("the elevated helper did not start in time".to_string());
        }
        std::thread::sleep(POLL);
    }
    *guard = Some(Helper { process });
    Ok(())
}

/// Keeps `alive` in the shared folder fresh while this app runs, so the helper stays up.
fn start_heartbeat(dir: &Path) {
    HEARTBEAT.get_or_init(|| {
        let dir = dir.to_path_buf();
        std::thread::spawn(move || {
            loop {
                let _ = std::fs::write(dir.join("alive"), "");
                std::thread::sleep(HEARTBEAT_EVERY);
            }
        });
    });
}

fn helper_exited() -> bool {
    HELPER
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().map(|helper| helper.process.has_exited()))
        .unwrap_or(true)
}

fn wait_for_output(paths: &FastPaths) -> Result<(), String> {
    loop {
        if paths.output.exists() {
            return Ok(());
        }
        if helper_exited() {
            return Err("the elevated helper stopped before it answered".to_string());
        }
        std::thread::sleep(POLL);
    }
}

fn read_result(paths: &FastPaths) -> Result<(VolumeSummary, FileTree), String> {
    let file = std::fs::File::open(&paths.output)
        .map_err(|error| format!("could not read the helper answer: {error}"))?;
    let mut reader = BufReader::with_capacity(1 << 20, file);
    let mut head = String::new();
    loop {
        let mut line = String::new();
        let read = reader
            .read_line(&mut line)
            .map_err(|error| format!("could not read the helper answer: {error}"))?;
        if read == 0 {
            return Err(head
                .lines()
                .find_map(|line| line.strip_prefix("error="))
                .unwrap_or("the helper result was incomplete")
                .to_string());
        }
        if line == TREE_MARKER {
            break;
        }
        head.push_str(&line);
    }
    let summary = VolumeSummary::from_text(&head)
        .ok_or_else(|| "the helper summary was incomplete".to_string())?;
    let mut tree = timberstat_ntfs::read_tree(reader)
        .map_err(|error| format!("could not read the folder tree: {error}"))?;
    prune_skipped(&mut tree);
    Ok((summary, tree))
}

/// Removes the folders the normal scan skips, so the Fast result matches a walk. Totals are
/// rolled up again after the removal.
fn prune_skipped(tree: &mut FileTree) {
    let skip = timberstat_scanner::ScanOptions::default().skip_names;
    let doomed: Vec<timberstat_filesystem::NodeId> = tree
        .attached_ids()
        .filter(|id| {
            tree.node(*id)
                .is_some_and(|node| timberstat_scanner::is_skipped_name(node.name(), &skip))
        })
        .collect();
    for id in &doomed {
        let _ = tree.detach(*id);
    }
    if !doomed.is_empty() {
        tree.roll_up();
    }
}

/// Entry point for the elevated helper: runs the requests in its folder until the window stops
/// refreshing the heartbeat. `args[0]` is the shared folder.
pub fn service_main(args: &[String]) -> i32 {
    let Some(dir) = args.first().map(PathBuf::from) else {
        return 2;
    };
    let _ = std::fs::write(dir.join("ready"), "");
    loop {
        std::thread::sleep(POLL);
        if let Some((volume, output)) = take_request(&dir) {
            let in_folder = Path::new(&output).parent() == Some(dir.as_path());
            if valid_volume(&volume) && in_folder {
                let _ = helper_main(&[volume, output]);
            } else if in_folder {
                let _ = std::fs::write(&output, "error=the request was not valid\n");
            }
        }
        let idle = heartbeat_age(&dir).is_none_or(|age| age > HELPER_IDLE_LIMIT);
        if idle {
            break;
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    0
}

/// Reads and removes the request in the helper's folder, if there is one.
fn take_request(dir: &Path) -> Option<(String, String)> {
    let request = dir.join("request");
    let text = std::fs::read_to_string(&request).ok()?;
    let _ = std::fs::remove_file(&request);
    let mut lines = text.lines();
    let volume = lines.next()?.trim().to_string();
    let output = lines.next()?.trim().to_string();
    Some((volume, output))
}

/// True for `//./X:` where X is a letter.
fn valid_volume(volume: &str) -> bool {
    let bytes = volume.as_bytes();
    bytes.len() == 6
        && volume.starts_with("//./")
        && bytes[4].is_ascii_alphabetic()
        && bytes[5] == b':'
}

fn heartbeat_age(dir: &Path) -> Option<Duration> {
    std::fs::metadata(dir.join("alive"))
        .ok()?
        .modified()
        .ok()?
        .elapsed()
        .ok()
}

/// Performs one scan for the helper. Writes `started` first, the progress file while reading,
/// then the tree and the summary. The summary is written last, under its final name, so the
/// window only reads a finished answer.
pub fn helper_main(args: &[String]) -> i32 {
    let (Some(volume), Some(output)) = (args.first(), args.get(1)) else {
        return 2;
    };
    let output = PathBuf::from(output);
    let progress_path = output.with_extension("progress");
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
    let temp = output.with_extension("writing");
    let saved = match result {
        Ok((summary, tree)) => save_result(&temp, &summary, &tree).or_else(|error| {
            std::fs::write(&temp, format!("error=could not save the result: {error}\n"))
        }),
        Err(error) => std::fs::write(
            &temp,
            format!("error={}\n", error.to_string().replace('\n', " ")),
        ),
    };
    match saved.and_then(|()| std::fs::rename(&temp, &output)) {
        Ok(()) => 0,
        Err(_) => 3,
    }
}

/// Writes the summary, the tree marker and the tree into one file, in that order.
fn save_result(path: &Path, summary: &VolumeSummary, tree: &FileTree) -> std::io::Result<()> {
    let mut file = std::io::BufWriter::with_capacity(1 << 20, std::fs::File::create(path)?);
    file.write_all(summary.to_text().as_bytes())?;
    file.write_all(TREE_MARKER.as_bytes())?;
    timberstat_ntfs::write_tree(tree, &mut file)?;
    file.flush()
}

/// One line for the progress file: `done total`, or `build`.
fn stage_line(stage: SummaryProgress) -> String {
    match stage {
        SummaryProgress::Reading { done, total } => format!("{done} {total}\n"),
        SummaryProgress::Building => "build\n".to_string(),
    }
}

/// Reads the latest stage the helper wrote. `None` until the helper has written one.
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
    fn only_drive_volumes_are_accepted_by_the_helper() {
        assert!(valid_volume("//./C:"));
        assert!(valid_volume("//./w:"));
        assert!(!valid_volume("//./C:\\Windows"));
        assert!(!valid_volume("C:"));
        assert!(!valid_volume("//./1:"));
    }

    #[test]
    fn result_files_sit_beside_the_output() {
        let paths = FastPaths::for_this_run();
        assert_eq!(paths.started.with_extension("txt"), paths.output);
    }
}
