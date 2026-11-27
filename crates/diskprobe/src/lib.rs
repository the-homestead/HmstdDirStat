//! Asks Windows whether a drive is a spinning hard disk, with no PowerShell and no administrator
//! rights.
//!
//! The volume `\.\X:` is opened with no data access, then `IOCTL_STORAGE_QUERY_PROPERTY` asks
//! for the seek-penalty descriptor (`StorageDeviceSeekPenaltyProperty`). A device that incurs a
//! seek penalty is a spinning disk. Docs:
//! <https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ni-winioctl-ioctl_storage_query_property>
//! and <https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ns-winioctl-device_seek_penalty_descriptor>.

/// `Some(true)` for a spinning disk, `Some(false)` for a device without seek penalty (SSD),
/// `None` when the drive cannot be asked, or on systems without this query.
pub fn seek_penalty(letter: char) -> Option<bool> {
    imp::seek_penalty(letter)
}

#[cfg(windows)]
mod imp {
    use std::mem::size_of;
    use std::ptr::{null, null_mut};

    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;
    use windows_sys::Win32::System::Ioctl::{
        DEVICE_SEEK_PENALTY_DESCRIPTOR, IOCTL_STORAGE_QUERY_PROPERTY, PropertyStandardQuery,
        STORAGE_PROPERTY_QUERY, StorageDeviceSeekPenaltyProperty,
    };

    pub fn seek_penalty(letter: char) -> Option<bool> {
        if !letter.is_ascii_alphabetic() {
            return None;
        }
        // `\.\W:` names the volume itself. The NUL terminator is required by the wide API.
        let mut text = String::from(r"\\.\");
        text.push(letter.to_ascii_uppercase());
        text.push(':');
        let path: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();

        // SAFETY: `path` is a NUL-terminated wide string that outlives the call. Access 0 asks
        // for no data rights, so the open does not need administrator rights.
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                null(),
                OPEN_EXISTING,
                0,
                null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return None;
        }

        let query = STORAGE_PROPERTY_QUERY {
            PropertyId: StorageDeviceSeekPenaltyProperty,
            QueryType: PropertyStandardQuery,
            AdditionalParameters: [0],
        };
        let mut descriptor = DEVICE_SEEK_PENALTY_DESCRIPTOR {
            Version: 0,
            Size: 0,
            IncursSeekPenalty: false,
        };
        let mut returned: u32 = 0;
        let out_size = u32::try_from(size_of::<DEVICE_SEEK_PENALTY_DESCRIPTOR>()).ok()?;
        let in_size = u32::try_from(size_of::<STORAGE_PROPERTY_QUERY>()).ok()?;

        // SAFETY: the input and output buffers are live, correctly sized structures of the types
        // the control code expects, and the handle came from a successful CreateFileW.
        let ok = unsafe {
            DeviceIoControl(
                handle,
                IOCTL_STORAGE_QUERY_PROPERTY,
                (&query as *const STORAGE_PROPERTY_QUERY).cast(),
                in_size,
                (&mut descriptor as *mut DEVICE_SEEK_PENALTY_DESCRIPTOR).cast(),
                out_size,
                &mut returned,
                null_mut(),
            )
        };
        // SAFETY: `handle` is valid and is closed exactly once, here.
        unsafe { CloseHandle(handle) };

        (ok != 0 && returned >= out_size).then_some(descriptor.IncursSeekPenalty)
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn seek_penalty(_letter: char) -> Option<bool> {
        None
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn non_letters_are_never_asked() {
        assert_eq!(super::seek_penalty('1'), None);
        assert_eq!(super::seek_penalty(' '), None);
    }
}

#[cfg(test)]
mod live_probe {
    /// Read-only timing on the real drives. Run with `--ignored --nocapture`.
    #[test]
    #[ignore]
    fn time_real_drives() {
        for letter in ['C', 'A', 'W'] {
            let started = std::time::Instant::now();
            let answer = super::seek_penalty(letter);
            println!(
                "drive {letter}: seek_penalty={answer:?} took {:.3}ms",
                started.elapsed().as_secs_f64() * 1000.0
            );
        }
    }
}
