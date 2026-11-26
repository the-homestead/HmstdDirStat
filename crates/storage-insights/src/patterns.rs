//! Path and file-name heuristics. These identify review candidates, not facts
//! about whether content is wanted.

use std::{ffi::OsStr, path::Path};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SegmentFlags {
    pub browser: bool,
    pub cache: bool,
    pub logs: bool,
}

pub(crate) fn segment_flags(directory: &Path) -> SegmentFlags {
    let mut flags = SegmentFlags::default();
    for component in directory.components() {
        let segment = component.as_os_str().to_string_lossy().to_lowercase();
        flags.browser |= matches!(
            segment.as_str(),
            "chrome"
                | "chromium"
                | "edge"
                | "msedge"
                | "microsoft edge"
                | "firefox"
                | "mozilla"
                | "brave"
                | "opera"
                | "vivaldi"
                | "safari"
                | "yandex"
        );
        flags.cache |= matches!(
            segment.as_str(),
            "cache"
                | "caches"
                | "code cache"
                | "gpucache"
                | "gpu cache"
                | "shadercache"
                | "dawncache"
                | "cachestorage"
                | "webkitcache"
                | "http cache"
                | "disk cache"
                | "cache2"
        );
        flags.logs |= matches!(segment.as_str(), "log" | "logs");
    }
    flags
}

pub(crate) fn lower_name(name: &OsStr) -> String {
    name.to_string_lossy().to_lowercase()
}

pub(crate) fn extension_of(name: &OsStr) -> String {
    Path::new(name)
        .extension()
        .map_or_else(String::new, |extension| {
            format!(".{}", extension.to_string_lossy().to_lowercase())
        })
}

pub(crate) fn is_vm_extension(extension: &str) -> bool {
    matches!(extension, ".vhd" | ".vhdx" | ".vmdk" | ".qcow2" | ".vdi")
}

pub(crate) fn is_crash_dump_name(name: &str) -> bool {
    matches!(
        Path::new(name)
            .extension()
            .map(|value| value.to_string_lossy().to_lowercase())
            .as_deref(),
        Some("dmp" | "mdmp" | "hdmp" | "wer" | "crash")
    )
}

pub(crate) fn is_installer_name(name: &str) -> bool {
    let extension = Path::new(name)
        .extension()
        .map(|value| value.to_string_lossy().to_lowercase());
    match extension.as_deref() {
        Some(
            "msi" | "msix" | "appx" | "pkg" | "deb" | "rpm" | "apk" | "appimage" | "dmg" | "ipk",
        ) => true,
        // A large executable is not necessarily an installer; require a name hint.
        Some("exe") => ["setup", "install"].iter().any(|hint| name.contains(hint)),
        _ => false,
    }
}

pub(crate) fn is_archive_name(name: &str) -> bool {
    [
        ".zip", ".tar", ".tar.gz", ".tgz", ".tar.xz", ".txz", ".tar.zst", ".7z", ".rar", ".gz",
        ".bz2", ".xz", ".zst", ".cab", ".lz4", ".lzh", ".arj",
    ]
    .iter()
    .any(|suffix| name.ends_with(suffix))
}

pub(crate) fn is_log_name(name: &str) -> bool {
    if name.ends_with(".log") || name.ends_with(".out") || name.ends_with(".err") {
        return true;
    }
    name.rsplit_once(".log.").is_some_and(|(_, rotation)| {
        !rotation.is_empty() && rotation.bytes().all(|byte| byte.is_ascii_digit())
    })
}

pub(crate) fn is_media_extension(extension: &str) -> bool {
    matches!(
        extension,
        ".mp4"
            | ".mkv"
            | ".avi"
            | ".mov"
            | ".wmv"
            | ".flv"
            | ".webm"
            | ".m4v"
            | ".mpg"
            | ".mpeg"
            | ".mp3"
            | ".flac"
            | ".wav"
            | ".aac"
            | ".ogg"
            | ".m4a"
            | ".jpg"
            | ".jpeg"
            | ".png"
            | ".gif"
            | ".bmp"
            | ".tiff"
            | ".heic"
            | ".raw"
    )
}
