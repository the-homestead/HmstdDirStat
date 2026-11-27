//! Handler catalog: which archive executables actually exist on this host.

use std::collections::HashSet;
use std::path::PathBuf;

use crate::format::ArchiveFormat;
use crate::handler_detect::{candidates, display_name, known_extra_paths, locate, verify};

/// Families of archive programs sharing a command-line surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HandlerFamily {
    SevenZip,
    Tar,
    Unzip,
    Zip,
    Gzip,
    Bzip2,
    Xz,
    Zstd,
    UnRar,
}

impl HandlerFamily {
    pub fn id(self) -> &'static str {
        match self {
            HandlerFamily::SevenZip => "sevenzip",
            HandlerFamily::Tar => "tar",
            HandlerFamily::Unzip => "unzip",
            HandlerFamily::Zip => "zip",
            HandlerFamily::Gzip => "gzip",
            HandlerFamily::Bzip2 => "bzip2",
            HandlerFamily::Xz => "xz",
            HandlerFamily::Zstd => "zstd",
            HandlerFamily::UnRar => "unrar",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            HandlerFamily::SevenZip => "7-Zip",
            HandlerFamily::Tar => "tar",
            HandlerFamily::Unzip => "unzip",
            HandlerFamily::Zip => "zip",
            HandlerFamily::Gzip => "gzip",
            HandlerFamily::Bzip2 => "bzip2",
            HandlerFamily::Xz => "xz",
            HandlerFamily::Zstd => "zstd",
            HandlerFamily::UnRar => "unrar",
        }
    }

    pub fn read_formats(self) -> &'static [ArchiveFormat] {
        match self {
            HandlerFamily::SevenZip => &ArchiveFormat::ALL,
            HandlerFamily::Tar => &[
                ArchiveFormat::Tar,
                ArchiveFormat::TarGz,
                ArchiveFormat::TarXz,
                ArchiveFormat::TarZst,
            ],
            HandlerFamily::Unzip => &[ArchiveFormat::Zip],
            HandlerFamily::Zip => &[],
            HandlerFamily::Gzip => &[ArchiveFormat::Gzip],
            HandlerFamily::Bzip2 => &[ArchiveFormat::Bzip2],
            HandlerFamily::Xz => &[ArchiveFormat::Xz],
            HandlerFamily::Zstd => &[ArchiveFormat::Zstandard],
            HandlerFamily::UnRar => &[ArchiveFormat::Rar],
        }
    }

    pub fn create_formats(self) -> &'static [ArchiveFormat] {
        match self {
            HandlerFamily::SevenZip => &[
                ArchiveFormat::Zip,
                ArchiveFormat::SevenZip,
                ArchiveFormat::Tar,
                ArchiveFormat::Gzip,
                ArchiveFormat::Bzip2,
                ArchiveFormat::Xz,
                ArchiveFormat::Zstandard,
            ],
            HandlerFamily::Tar => &[
                ArchiveFormat::Tar,
                ArchiveFormat::TarGz,
                ArchiveFormat::TarXz,
                ArchiveFormat::TarZst,
            ],
            HandlerFamily::Zip => &[ArchiveFormat::Zip],
            _ => &[],
        }
    }

    pub fn can_test(self) -> bool {
        !matches!(self, HandlerFamily::Zip)
    }

    pub fn can_add(self) -> bool {
        matches!(
            self,
            HandlerFamily::SevenZip | HandlerFamily::Tar | HandlerFamily::Zip
        )
    }

    pub fn can_read(self, format: ArchiveFormat) -> bool {
        self.read_formats().contains(&format)
    }
}

/// Whether a program may be launched by the app at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandlerKind {
    /// Console program, safe to run headless.
    Console,
    /// GUI program (NanaZip file manager, WinRAR). Detected only; never launched.
    GuiOnly,
}

/// One program that was found and verified on this host.
#[derive(Debug, Clone)]
pub struct HandlerSpec {
    pub family: HandlerFamily,
    pub kind: HandlerKind,
    pub program: PathBuf,
    pub name: String,
    pub banner: Option<String>,
}

impl HandlerSpec {
    pub fn id(&self) -> &'static str {
        self.family.id()
    }

    pub fn usable(&self) -> bool {
        self.kind == HandlerKind::Console
    }

    pub fn describe(&self) -> String {
        format!("{} [{}]", self.name, self.program.display())
    }
}

/// Directories and explicit paths used for detection. Injectable so tests can
/// point at a temp directory of fake executables.
#[derive(Debug, Clone, Default)]
pub struct SearchPaths {
    pub dirs: Vec<PathBuf>,
    pub extra: Vec<PathBuf>,
}

impl SearchPaths {
    pub fn from_env() -> Self {
        let dirs = std::env::var_os("PATH")
            .map(|value| std::env::split_paths(&value).collect())
            .unwrap_or_default();
        Self {
            dirs,
            extra: known_extra_paths(),
        }
    }

    pub fn new(dirs: Vec<PathBuf>) -> Self {
        Self {
            dirs,
            extra: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct HandlerCatalog {
    handlers: Vec<HandlerSpec>,
    rejected: Vec<String>,
    preferred_id: Option<&'static str>,
}

impl HandlerCatalog {
    pub fn detect() -> Self {
        Self::detect_with(&SearchPaths::from_env())
    }

    pub fn detect_with(paths: &SearchPaths) -> Self {
        let mut handlers: Vec<HandlerSpec> = Vec::new();
        let mut rejected: Vec<String> = Vec::new();

        for candidate in candidates() {
            if handlers
                .iter()
                .any(|handler| handler.family == candidate.family && handler.usable())
            {
                continue;
            }
            let Some(program) = locate(&candidate, paths) else {
                continue;
            };
            if candidate.kind == HandlerKind::GuiOnly {
                let name = display_name(candidate.family, &program, None);
                handlers.push(HandlerSpec {
                    family: candidate.family,
                    kind: HandlerKind::GuiOnly,
                    program,
                    name,
                    banner: None,
                });
                continue;
            }
            match verify(candidate.family, &program) {
                Some(banner) => {
                    let name = display_name(candidate.family, &program, banner.as_deref());
                    handlers.push(HandlerSpec {
                        family: candidate.family,
                        kind: HandlerKind::Console,
                        program,
                        name,
                        banner,
                    });
                }
                None => rejected.push(format!(
                    "{} at {} did not answer its version command",
                    candidate.family.label(),
                    program.display()
                )),
            }
        }

        Self {
            handlers,
            rejected,
            preferred_id: None,
        }
    }

    pub fn with_preferred(mut self, id: Option<&'static str>) -> Self {
        self.preferred_id = id;
        self
    }

    pub fn preferred(&self) -> Option<&'static str> {
        self.preferred_id
    }

    pub fn handlers(&self) -> &[HandlerSpec] {
        &self.handlers
    }

    pub fn rejected(&self) -> &[String] {
        &self.rejected
    }

    pub fn usable(&self) -> impl Iterator<Item = &HandlerSpec> {
        self.handlers.iter().filter(|handler| handler.usable())
    }

    pub fn missing_formats(&self) -> Vec<ArchiveFormat> {
        let covered: HashSet<ArchiveFormat> = self
            .usable()
            .flat_map(|handler| handler.family.read_formats().iter().copied())
            .collect();
        ArchiveFormat::ALL
            .iter()
            .copied()
            .filter(|format| !covered.contains(format))
            .collect()
    }

    pub fn missing_summary(&self) -> Option<String> {
        let missing = self.missing_formats();
        if missing.is_empty() {
            return None;
        }
        let labels: Vec<&str> = missing.iter().map(|format| format.label()).collect();
        Some(format!("no handler: {}", labels.join(", ")))
    }

    pub fn missing_message(&self, format: ArchiveFormat) -> Option<String> {
        let covered = self.usable().any(|handler| handler.family.can_read(format));
        if covered {
            None
        } else {
            Some(format!("no handler: {}", format.label()))
        }
    }

    pub fn read_handler(&self, format: ArchiveFormat) -> Option<&HandlerSpec> {
        if let Some(preferred) = self.preferred_id {
            return self
                .usable()
                .find(|handler| handler.id() == preferred && handler.family.can_read(format));
        }
        let preferred_order = [
            HandlerFamily::SevenZip.id(),
            HandlerFamily::Tar.id(),
            HandlerFamily::Unzip.id(),
            HandlerFamily::Gzip.id(),
            HandlerFamily::Bzip2.id(),
            HandlerFamily::Xz.id(),
            HandlerFamily::Zstd.id(),
            HandlerFamily::UnRar.id(),
        ];
        for family in preferred_order {
            let Some(handler) = self.usable().find(|handler| handler.id() == family) else {
                continue;
            };
            if handler.family.can_read(format) {
                return Some(handler);
            }
        }
        None
    }

    pub fn create_handler(&self, format: ArchiveFormat) -> Option<&HandlerSpec> {
        if let Some(preferred) = self.preferred_id {
            return self
                .usable()
                .find(|handler| handler.id() == preferred && handler.family.can_read(format));
        }
        let preferred_order = [
            HandlerFamily::SevenZip.id(),
            HandlerFamily::Tar.id(),
            HandlerFamily::Zip.id(),
        ];
        for family in preferred_order {
            let Some(handler) = self.usable().find(|handler| handler.id() == family) else {
                continue;
            };
            if handler.family.can_read(format) {
                return Some(handler);
            }
        }
        None
    }

    pub fn preferred_is_missing(&self, format: ArchiveFormat) -> bool {
        match self.preferred_id {
            Some(preferred) => !self
                .usable()
                .any(|handler| handler.id() == preferred && handler.family.can_read(format)),
            None => false,
        }
    }
}
