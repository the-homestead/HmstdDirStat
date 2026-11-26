//! Opens a folder in the system file manager.

use std::{io, path::Path, process::Command};

/// Opens Explorer with `path` selected inside its folder. Windows only; other systems open
/// the containing folder.
pub fn select_in_folder(path: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut select = std::ffi::OsString::from("/select,");
        select.push(path.as_os_str());
        Command::new("explorer.exe")
            .arg(select)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()?;
    }
    #[cfg(not(windows))]
    {
        let folder = path.parent().unwrap_or(path);
        open_folder(folder)?;
    }
    Ok(())
}

/// Creates `path` if needed and opens it. The child process is not waited for.
pub fn open_folder(path: &Path) -> io::Result<()> {
    std::fs::create_dir_all(path)?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        Command::new("explorer.exe")
            .arg(path)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()?;
    }
    #[cfg(target_os = "macos")]
    Command::new("open").arg(path).spawn()?;
    #[cfg(all(unix, not(target_os = "macos")))]
    Command::new("xdg-open").arg(path).spawn()?;
    Ok(())
}
