//! Starts this program elevated through the Windows consent prompt (`runas`), and keeps the
//! process handle so the caller can tell when the elevated process has gone.
//!
//! Docs: <https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shellexecuteexw>.

use std::path::Path;

/// Why the elevated process did not start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchError {
    /// The user refused the consent prompt (`ERROR_CANCELLED`).
    Declined,
    /// Any other failure, with the Windows error code (0 when the platform has no elevation).
    Failed(u32),
}

/// A launched elevated process. Dropping it closes the handle; the process keeps running.
pub struct Elevated {
    #[cfg(windows)]
    process: isize,
}

#[cfg(windows)]
mod imp {
    use std::ffi::OsStr;
    use std::iter::once;
    use std::mem::{size_of, zeroed};
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::WaitForSingleObject;
    use windows_sys::Win32::UI::Shell::{
        SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW,
    };

    use super::{Elevated, LaunchError};

    /// `ERROR_CANCELLED`: the consent prompt was declined.
    const ERROR_CANCELLED: u32 = 1223;

    /// A NUL-terminated UTF-16 copy of `text`, as the wide API expects.
    fn wide(text: &OsStr) -> Vec<u16> {
        text.encode_wide().chain(once(0)).collect()
    }

    pub fn launch(exe: &Path, arguments: &str) -> Result<Elevated, LaunchError> {
        let verb = wide(OsStr::new("runas"));
        let file = wide(exe.as_os_str());
        let parameters = wide(OsStr::new(arguments));
        // SAFETY: all-zero is a valid SHELLEXECUTEINFOW; every field the call reads is set below.
        let mut info: SHELLEXECUTEINFOW = unsafe { zeroed() };
        info.cbSize = u32::try_from(size_of::<SHELLEXECUTEINFOW>()).unwrap_or(0);
        info.fMask = SEE_MASK_NOCLOSEPROCESS;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = parameters.as_ptr();
        info.nShow = 0; // SW_HIDE: no window of the helper is shown.

        // SAFETY: `info` is initialised, and the wide strings it points to stay alive until the
        // call returns.
        let ok = unsafe { ShellExecuteExW(&mut info) };
        if ok == 0 {
            // SAFETY: reads the calling thread's last error; no arguments.
            let code = unsafe { GetLastError() };
            return Err(if code == ERROR_CANCELLED {
                LaunchError::Declined
            } else {
                LaunchError::Failed(code)
            });
        }
        if info.hProcess.is_null() {
            return Err(LaunchError::Failed(0));
        }
        Ok(Elevated {
            process: info.hProcess as isize,
        })
    }

    impl Elevated {
        pub fn has_exited(&self) -> bool {
            // SAFETY: the handle was returned by ShellExecuteExW and is closed only in Drop.
            unsafe { WaitForSingleObject(self.process as _, 0) == WAIT_OBJECT_0 }
        }
    }

    impl Drop for Elevated {
        fn drop(&mut self) {
            // SAFETY: closes the handle this value owns, exactly once.
            unsafe { CloseHandle(self.process as _) };
        }
    }
}

#[cfg(windows)]
pub fn launch(exe: &Path, arguments: &str) -> Result<Elevated, LaunchError> {
    imp::launch(exe, arguments)
}

#[cfg(not(windows))]
pub fn launch(_exe: &Path, _arguments: &str) -> Result<Elevated, LaunchError> {
    Err(LaunchError::Failed(0))
}

#[cfg(not(windows))]
impl Elevated {
    pub fn has_exited(&self) -> bool {
        true
    }
}
