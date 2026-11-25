use std::{io, path::PathBuf};

/// Lists scannable volume roots on this machine.
pub trait VolumeEnumerator {
    fn volumes(&self) -> io::Result<Vec<PathBuf>>;
}

/// Volumes of the running system.
///
/// Windows: probes drive letters `A:`–`Z:`. Probing an empty removable drive can
/// make Windows show a "no disk" prompt; a `GetLogicalDrives` bitmask avoids that
/// and is planned with the Windows platform crate (Phase 3).
///
/// Linux: mount points from `/proc/self/mountinfo`, deduplicated.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemVolumes;

impl VolumeEnumerator for SystemVolumes {
    #[cfg(windows)]
    fn volumes(&self) -> io::Result<Vec<PathBuf>> {
        let mut roots = Vec::new();
        for letter in b'A'..=b'Z' {
            let root = PathBuf::from(format!("{}:\\", char::from(letter)));
            if root.is_dir() {
                roots.push(root);
            }
        }
        Ok(roots)
    }

    #[cfg(target_os = "linux")]
    fn volumes(&self) -> io::Result<Vec<PathBuf>> {
        let text = std::fs::read_to_string("/proc/self/mountinfo")?;
        let mut mounts: Vec<PathBuf> = Vec::new();
        for line in text.lines() {
            // Field 5 (1-based) is the mount point; octal escapes such as \040 encode spaces.
            let Some(raw) = line.split(' ').nth(4) else {
                continue;
            };
            let mount = PathBuf::from(unescape_mountinfo(raw));
            if !mounts.contains(&mount) {
                mounts.push(mount);
            }
        }
        Ok(mounts)
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    fn volumes(&self) -> io::Result<Vec<PathBuf>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "volume enumeration is not implemented on this platform",
        ))
    }
}

/// Decodes the `\ooo` octal escapes used in `/proc/self/mountinfo`.
#[cfg(any(test, target_os = "linux"))]
fn unescape_mountinfo(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\'
            && i + 3 < bytes.len()
            && bytes[i + 1..i + 4]
                .iter()
                .all(|b| (b'0'..=b'7').contains(b))
        {
            let value =
                (bytes[i + 1] - b'0') * 64 + (bytes[i + 2] - b'0') * 8 + (bytes[i + 3] - b'0');
            out.push(value);
            i += 4;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unescape_decodes_octal_sequences() {
        assert_eq!(unescape_mountinfo("/mnt/my\\040disk"), "/mnt/my disk");
        assert_eq!(unescape_mountinfo("/plain"), "/plain");
        assert_eq!(unescape_mountinfo("/a\\134b"), "/a\\b");
    }

    #[test]
    fn unescape_leaves_truncated_escape_alone() {
        assert_eq!(unescape_mountinfo("/x\\04"), "/x\\04");
    }

    #[cfg(windows)]
    #[test]
    fn windows_lists_the_system_drive() {
        let volumes = SystemVolumes.volumes().unwrap();
        let system = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        assert!(
            volumes.iter().any(|v| v
                .to_string_lossy()
                .to_uppercase()
                .starts_with(&system.to_uppercase())),
            "expected {system} in {volumes:?}"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_lists_root_mount() {
        let volumes = SystemVolumes.volumes().unwrap();
        assert!(volumes.iter().any(|v| v == std::path::Path::new("/")));
    }
}
