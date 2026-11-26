//! Real `argv` process execution with timeout and cancellation.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use hmstd_fileops::Control;

/// Captured result of one handler invocation.
#[derive(Debug, Clone, Default)]
pub struct ProcessOutcome {
    pub program: PathBuf,
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration: Duration,
    pub timed_out: bool,
    pub cancelled: bool,
}

fn spawn_reader<R: Read + Send + 'static>(mut reader: R) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = reader.read_to_end(&mut buffer);
        let _ = tx.send(buffer);
    });
    rx
}

fn kill_child(mut child: Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Run an archive handler as a real argv process.
///
/// No shell string is ever used. stdout/stderr are captured, and the call is
/// bounded by `timeout`. Cancellation is checked while waiting; on timeout or
/// cancellation the child is terminated and the outcome is reported instead of
/// blocking forever.
pub fn run(
    program: &Path,
    args: &[String],
    cwd: Option<&Path>,
    timeout: Duration,
    control: Option<&Control>,
) -> Result<ProcessOutcome, crate::error::ArchiveError> {
    let started = Instant::now();
    let mut command = Command::new(program);
    command.args(args.iter().map(|s| s.as_str()));
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }
    let mut child: Child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| crate::error::ArchiveError::io_error(program, error))?;

    let out_pipe = child.stdout.take().expect("stdout piped");
    let err_pipe = child.stderr.take().expect("stderr piped");
    let out_rx = spawn_reader(out_pipe);
    let err_rx = spawn_reader(err_pipe);

    let deadline = Instant::now() + timeout;
    let mut status: Option<i32> = None;
    let mut timed_out = false;
    let mut cancelled = false;

    loop {
        if let Some(ctrl) = control
            && ctrl.is_cancelled()
        {
            cancelled = true;
            break;
        }
        match child.try_wait() {
            Ok(Some(current_status)) => {
                status = Some(current_status.code().unwrap_or(-1));
                break;
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    timed_out = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(error) => {
                return Err(crate::error::ArchiveError::io_error(
                    program,
                    std::io::Error::other(error.to_string()),
                ));
            }
        }
    }

    if timed_out || cancelled {
        kill_child(child);
    }

    let final_status = if timed_out || cancelled { None } else { status };
    let final_cancelled = cancelled;

    let stdout_bytes = out_rx.recv().unwrap_or_default();
    let stderr_bytes = err_rx.recv().unwrap_or_default();
    let status = if timed_out || cancelled {
        status
    } else {
        final_status
    };

    Ok(ProcessOutcome {
        program: program.to_path_buf(),
        status,
        stdout: String::from_utf8_lossy(&stdout_bytes).into_owned(),
        stderr: String::from_utf8_lossy(&stderr_bytes).into_owned(),
        duration: started.elapsed(),
        timed_out,
        cancelled: final_cancelled,
    })
}
