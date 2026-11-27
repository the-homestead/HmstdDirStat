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
/// Linux: mount points from `/proc/self/mountinfo`, deduplicated, without kernel pseudo
/// filesystems (proc, sysfs, cgroup, ...). See `mounts_from_mountinfo`.
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
        Ok(mounts_from_mountinfo(&text))
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    fn volumes(&self) -> io::Result<Vec<PathBuf>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "volume enumeration is not implemented on this platform",
        ))
    }
}

/// Filesystem types that describe the kernel rather than stored data. Scanning them yields
/// nothing useful, so they are not offered as volumes.
#[cfg(any(test, target_os = "linux"))]
const PSEUDO_FILESYSTEMS: &[&str] = &[
    "proc",
    "sysfs",
    "devtmpfs",
    "devpts",
    "cgroup",
    "cgroup2",
    "pstore",
    "bpf",
    "tracefs",
    "debugfs",
    "securityfs",
    "configfs",
    "fusectl",
    "hugetlbfs",
    "mqueue",
    "autofs",
    "nsfs",
    "rpc_pipefs",
    "efivarfs",
    "binfmt_misc",
];

/// Parses `/proc/self/mountinfo` text into deduplicated mount points, skipping pseudo
/// filesystems. Each line is `id parent major:minor root mountpoint opts... - fstype source opts`.
#[cfg(any(test, target_os = "linux"))]
fn mounts_from_mountinfo(text: &str) -> Vec<PathBuf> {
    let mut mounts: Vec<PathBuf> = Vec::new();
    for line in text.lines() {
        if is_pseudo_mount(line) {
            continue;
        }
        // Field 5 (1-based) is the mount point; octal escapes such as \040 encode spaces.
        let Some(raw) = line.split(' ').nth(4) else {
            continue;
        };
        let mount = PathBuf::from(unescape_mountinfo(raw));
        if !mounts.contains(&mount) {
            mounts.push(mount);
        }
    }
    mounts
}

/// True when the filesystem type (the word after the ` - ` separator) is a pseudo type.
#[cfg(any(test, target_os = "linux"))]
fn is_pseudo_mount(line: &str) -> bool {
    line.split(" - ")
        .nth(1)
        .and_then(|tail| tail.split(' ').next())
        .is_some_and(|fstype| PSEUDO_FILESYSTEMS.contains(&fstype))
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

    /// Real-shaped mountinfo lines: root ext4 (twice, as bind mounts do), kernel pseudo mounts,
    /// and a vfat mount whose path has an escaped space.
    const SAMPLE: &str = concat!(
        "22 1 8:1 / / rw,relatime shared:1 - ext4 /dev/sda1 rw\n",
        "23 22 0:6 / /proc rw,nosuid shared:12 - proc proc rw\n",
        "24 22 0:21 / /sys/fs/cgroup ro shared:5 - cgroup2 cgroup2 rw\n",
        "25 22 8:2 / /mnt/my\\040disk rw shared:9 - vfat /dev/sdb1 rw\n",
        "26 22 8:1 / / rw,relatime shared:1 - ext4 /dev/sda1 rw\n",
    );

    #[test]
    fn mountinfo_skips_pseudo_filesystems_and_duplicates() {
        assert_eq!(
            mounts_from_mountinfo(SAMPLE),
            vec![PathBuf::from("/"), PathBuf::from("/mnt/my disk")]
        );
    }

    #[test]
    fn pseudo_detection_reads_the_type_after_the_separator() {
        assert!(is_pseudo_mount("23 22 0:6 / /proc rw - proc proc rw"));
        assert!(!is_pseudo_mount("22 1 8:1 / / rw - ext4 /dev/sda1 rw"));
        // A mount point named like a pseudo type is not a pseudo filesystem.
        assert!(!is_pseudo_mount(
            "30 22 8:3 /proc /mnt/proc rw - ext4 /dev/sdc1 rw"
        ));
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
