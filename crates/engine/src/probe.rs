//! Whether a drive is a spinning hard disk. Windows reports the media type of the physical disk
//! behind a drive letter; the query needs no administrator rights. Other systems, failed queries
//! and drives Windows cannot classify all answer "not a spinning disk", so they are walked.

use std::path::Path;
use std::process::Command;
use std::time::Instant;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Stops the PowerShell query from opening a console window.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// True when the drive's physical disk reports the `HDD` media type.
pub fn is_spinning_hdd(drive: &Path) -> bool {
    if !cfg!(windows) {
        return false;
    }
    let Some(letter) = drive
        .to_string_lossy()
        .chars()
        .next()
        .filter(char::is_ascii_alphabetic)
    else {
        return false;
    };
    let started = Instant::now();
    let script = format!(
        "(Get-Partition -DriveLetter {} | Get-Disk | Get-PhysicalDisk).MediaType",
        letter.to_ascii_uppercase()
    );
    let mut command = Command::new("powershell.exe");
    command.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let answer = command
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_default();
    tracing::info!(
        "{}",
        timberstat_core::timing::line("detect drive kind", started.elapsed())
    );
    answer == "HDD"
}
