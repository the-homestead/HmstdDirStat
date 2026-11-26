/// Archive format identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ArchiveFormat {
    /// ZIP (RFC 1951 / PKZIP).
    Zip,
    /// 7z / 7-Zip.
    SevenZip,
    /// RAR.
    Rar,
    /// TAR (uncompressed).
    Tar,
    /// TAR.GZ / TAR.GZIP.
    TarGz,
    /// TAR.XZ.
    TarXz,
    /// TAR.ZST / TAR.ZSTD.
    TarZst,
    /// GZIP (single-file compressed stream).
    Gzip,
    /// BZIP2 (single-file compressed stream).
    Bzip2,
    /// XZ (single-file compressed stream).
    Xz,
    /// Zstandard (single-file compressed stream).
    Zstandard,
}

impl ArchiveFormat {
    /// All known formats in preferred order.
    pub const ALL: [ArchiveFormat; 11] = [
        ArchiveFormat::Zip,
        ArchiveFormat::SevenZip,
        ArchiveFormat::Rar,
        ArchiveFormat::Tar,
        ArchiveFormat::TarGz,
        ArchiveFormat::TarXz,
        ArchiveFormat::TarZst,
        ArchiveFormat::Gzip,
        ArchiveFormat::Bzip2,
        ArchiveFormat::Xz,
        ArchiveFormat::Zstandard,
    ];

    /// Human-readable label for this format (used in UI / diagnostics).
    pub fn label(self) -> &'static str {
        match self {
            ArchiveFormat::Zip => "ZIP",
            ArchiveFormat::SevenZip => "7Z",
            ArchiveFormat::Rar => "RAR",
            ArchiveFormat::Tar => "TAR",
            ArchiveFormat::TarGz => "TAR.GZ",
            ArchiveFormat::TarXz => "TAR.XZ",
            ArchiveFormat::TarZst => "TAR.ZST",
            ArchiveFormat::Gzip => "GZIP",
            ArchiveFormat::Bzip2 => "BZIP2",
            ArchiveFormat::Xz => "XZ",
            ArchiveFormat::Zstandard => "ZSTANDARD",
        }
    }

    /// Is this a single‑file compressed stream (gzip/bzip2/xz/zstd) rather than a container?
    pub fn is_stream(self) -> bool {
        matches!(
            self,
            ArchiveFormat::Gzip
                | ArchiveFormat::Bzip2
                | ArchiveFormat::Xz
                | ArchiveFormat::Zstandard
        )
    }

    /// Return the format with the given extension, if any.
    /// Looks for compound extensions (`.tar.gz`, `.tgz`, `.tar.xz`, `.txz`,
    /// `.tar.zst`, `.tzst`) first, then the final extension.
    pub fn from_path(path: &std::path::Path) -> Option<ArchiveFormat> {
        let name = path.file_name()?.to_str()?.to_lowercase();

        if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
            return Some(ArchiveFormat::TarGz);
        }
        if name.ends_with(".tar.xz") || name.ends_with(".txz") {
            return Some(ArchiveFormat::TarXz);
        }
        if name.ends_with(".tar.zst") || name.ends_with(".tzst") {
            return Some(ArchiveFormat::TarZst);
        }

        let extension = path.extension()?.to_str()?;
        ArchiveFormat::from_extension(extension)
    }

    /// Try to infer a format from just the extension string.
    pub fn from_extension(ext: &str) -> Option<ArchiveFormat> {
        let ext = ext.trim_start_matches('.').to_lowercase();
        match ext.as_str() {
            "zip" => Some(ArchiveFormat::Zip),
            "7z" | "7za" | "7zf" => Some(ArchiveFormat::SevenZip),
            "rar" => Some(ArchiveFormat::Rar),
            "tar" => Some(ArchiveFormat::Tar),
            "gz" | "gzip" => Some(ArchiveFormat::Gzip),
            "bz2" | "bzip2" => Some(ArchiveFormat::Bzip2),
            "xz" => Some(ArchiveFormat::Xz),
            "zst" | "zstd" => Some(ArchiveFormat::Zstandard),
            _ => None,
        }
    }

    /// Returns the set of handler families that can handle this format.
    pub fn supported_families(&self) -> &'static [&'static str] {
        match self {
            ArchiveFormat::Zip => &["sevenzip", "unzip", "zip"],
            ArchiveFormat::SevenZip => &["sevenzip"],
            ArchiveFormat::Rar => &["sevenzip", "unrar"],
            ArchiveFormat::Tar => &["sevenzip", "tar"],
            ArchiveFormat::TarGz => &["sevenzip", "tar"],
            ArchiveFormat::TarXz => &["sevenzip", "tar"],
            ArchiveFormat::TarZst => &["sevenzip", "tar"],
            ArchiveFormat::Gzip => &["sevenzip", "gzip"],
            ArchiveFormat::Bzip2 => &["sevenzip", "bzip2"],
            ArchiveFormat::Xz => &["sevenzip", "xz"],
            ArchiveFormat::Zstandard => &["sevenzip", "zstd"],
        }
    }
}

/// Format detection from file extension, case‑insensitive.
pub fn detect_format(path: &std::path::Path) -> Option<ArchiveFormat> {
    ArchiveFormat::from_path(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_labels() {
        assert_eq!(ArchiveFormat::Zip.label(), "ZIP");
        assert_eq!(ArchiveFormat::Gzip.label(), "GZIP");
        assert_eq!(ArchiveFormat::TarZst.label(), "TAR.ZST");
    }

    #[test]
    fn test_is_stream() {
        assert!(ArchiveFormat::Gzip.is_stream());
        assert!(ArchiveFormat::Bzip2.is_stream());
        assert!(!ArchiveFormat::Tar.is_stream());
        assert!(!ArchiveFormat::Zip.is_stream());
    }

    #[test]
    fn test_from_path() {
        assert_eq!(
            ArchiveFormat::from_path(std::path::Path::new("archive.zip")),
            Some(ArchiveFormat::Zip)
        );
        assert_eq!(
            ArchiveFormat::from_path(std::path::Path::new("data.tar.gz")),
            Some(ArchiveFormat::TarGz)
        );
        assert_eq!(
            ArchiveFormat::from_path(std::path::Path::new("file.bz2")),
            Some(ArchiveFormat::Bzip2)
        );
        assert_eq!(
            ArchiveFormat::from_path(std::path::Path::new("unknown.xyz")),
            None
        );
    }

    #[test]
    fn test_from_extension() {
        assert_eq!(
            ArchiveFormat::from_extension("zip"),
            Some(ArchiveFormat::Zip)
        );
        assert_eq!(
            ArchiveFormat::from_extension("gz"),
            Some(ArchiveFormat::Gzip)
        );
        assert_eq!(
            ArchiveFormat::from_extension("tar"),
            Some(ArchiveFormat::Tar)
        );
        assert_eq!(ArchiveFormat::from_extension("xyz"), None);
    }

    #[test]
    fn test_supported_families() {
        // 7z can handle all formats through its family, but each format has preferred families.
        let families = ArchiveFormat::Zip.supported_families();
        assert!(families.contains(&"sevenzip"));
        assert!(families.contains(&"unzip"));
    }
}
