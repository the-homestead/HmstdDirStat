//! Filesystem detection of archive executables: candidate names, search and verification.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::handler::{HandlerFamily, HandlerKind, SearchPaths};
use crate::runner;

pub(crate) struct Candidate {
    pub(crate) family: HandlerFamily,
    pub(crate) kind: HandlerKind,
    pub(crate) names: &'static [&'static str],
    pub(crate) known_paths: Vec<PathBuf>,
}

pub(crate) fn known_extra_paths() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        for key in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(root) = std::env::var_os(key) {
                let root = PathBuf::from(root);
                paths.push(root.join("7-Zip").join("7z.exe"));
                paths.push(root.join("NanaZip").join("NanaZipC.exe"));
                paths.push(root.join("NanaZip").join("NanaZip.Universal.Console.exe"));
                paths.push(root.join("WinRAR").join("UnRAR.exe"));
                paths.push(root.join("WinRAR").join("WinRAR.exe"));
            }
        }
        paths.push(PathBuf::from(r"C:\Windows\System32\tar.exe"));
    }
    paths
}

pub(crate) fn candidates() -> Vec<Candidate> {
    let mut result = Vec::new();

    let mut sevenzip_paths = Vec::new();
    if cfg!(windows) {
        for key in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(root) = std::env::var_os(key) {
                let root = PathBuf::from(root);
                sevenzip_paths.push(root.join("7-Zip").join("7z.exe"));
                sevenzip_paths.push(root.join("NanaZip").join("NanaZipC.exe"));
                sevenzip_paths.push(root.join("NanaZip").join("NanaZip.Universal.Console.exe"));
            }
        }
    }
    result.push(Candidate {
        family: HandlerFamily::SevenZip,
        kind: HandlerKind::Console,
        names: &["7z", "7za", "7zz", "7zr"],
        known_paths: sevenzip_paths,
    });

    let mut tar_paths = Vec::new();
    if cfg!(windows) {
        tar_paths.push(PathBuf::from(r"C:\Windows\System32\tar.exe"));
    }
    result.push(Candidate {
        family: HandlerFamily::Tar,
        kind: HandlerKind::Console,
        names: &["tar"],
        known_paths: tar_paths,
    });

    result.push(Candidate {
        family: HandlerFamily::Unzip,
        kind: HandlerKind::Console,
        names: &["unzip"],
        known_paths: Vec::new(),
    });
    result.push(Candidate {
        family: HandlerFamily::Zip,
        kind: HandlerKind::Console,
        names: &["zip"],
        known_paths: Vec::new(),
    });
    result.push(Candidate {
        family: HandlerFamily::Gzip,
        kind: HandlerKind::Console,
        names: &["gzip"],
        known_paths: Vec::new(),
    });
    result.push(Candidate {
        family: HandlerFamily::Bzip2,
        kind: HandlerKind::Console,
        names: &["bzip2"],
        known_paths: Vec::new(),
    });
    result.push(Candidate {
        family: HandlerFamily::Xz,
        kind: HandlerKind::Console,
        names: &["xz"],
        known_paths: Vec::new(),
    });
    result.push(Candidate {
        family: HandlerFamily::Zstd,
        kind: HandlerKind::Console,
        names: &["zstd"],
        known_paths: Vec::new(),
    });

    let mut unrar_paths = Vec::new();
    if cfg!(windows) {
        for key in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(root) = std::env::var_os(key) {
                unrar_paths.push(PathBuf::from(root).join("WinRAR").join("UnRAR.exe"));
            }
        }
    }
    result.push(Candidate {
        family: HandlerFamily::UnRar,
        kind: HandlerKind::Console,
        names: &["unrar"],
        known_paths: unrar_paths,
    });

    let mut nanazip_gui = Vec::new();
    let mut winrar_gui = Vec::new();
    if cfg!(windows) {
        for key in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(root) = std::env::var_os(key) {
                let root = PathBuf::from(root);
                nanazip_gui.push(root.join("NanaZip").join("NanaZip.exe"));
                winrar_gui.push(root.join("WinRAR").join("WinRAR.exe"));
            }
        }
    }
    result.push(Candidate {
        family: HandlerFamily::SevenZip,
        kind: HandlerKind::GuiOnly,
        names: &["NanaZip"],
        known_paths: nanazip_gui,
    });
    result.push(Candidate {
        family: HandlerFamily::UnRar,
        kind: HandlerKind::GuiOnly,
        names: &["WinRAR"],
        known_paths: winrar_gui,
    });

    result
}

fn program_names(base: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        vec![
            base.to_owned(),
            format!("{base}.exe"),
            format!("{base}.cmd"),
            format!("{base}.bat"),
        ]
    }
    #[cfg(not(windows))]
    {
        vec![base.to_owned()]
    }
}

fn is_program_file(path: &Path) -> bool {
    path.is_file()
}

pub(crate) fn locate(candidate: &Candidate, paths: &SearchPaths) -> Option<PathBuf> {
    for path in &candidate.known_paths {
        if is_program_file(path) {
            return Some(path.clone());
        }
    }
    for dir in &paths.dirs {
        for base in candidate.names {
            for name in program_names(base) {
                let path = dir.join(name);
                if is_program_file(&path) {
                    return Some(path);
                }
            }
        }
    }
    for path in &paths.extra {
        if is_program_file(path) {
            return Some(path.clone());
        }
    }
    None
}

fn verify_argv(family: HandlerFamily) -> &'static [&'static str] {
    match family {
        HandlerFamily::SevenZip => &["i"],
        HandlerFamily::Tar => &["--version"],
        HandlerFamily::Unzip => &["-v"],
        HandlerFamily::Zip => &["-v"],
        HandlerFamily::Gzip => &["--version"],
        HandlerFamily::Bzip2 => &["--version"],
        HandlerFamily::Xz => &["--version"],
        HandlerFamily::Zstd => &["--version"],
        HandlerFamily::UnRar => &["-?"],
    }
}

fn banner_from(outcome: &runner::ProcessOutcome) -> Option<String> {
    for text in [&outcome.stdout, &outcome.stderr] {
        for line in text.lines() {
            let line = line.trim();
            if !line.is_empty() {
                return Some(line.to_owned());
            }
        }
    }
    None
}

pub(crate) fn verify(family: HandlerFamily, program: &Path) -> Option<Option<String>> {
    let args: Vec<String> = verify_argv(family)
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    match runner::run(program, &args, None, Duration::from_secs(10), None) {
        Ok(outcome) => {
            if outcome.cancelled || outcome.timed_out {
                return None;
            }
            let ok = if family == HandlerFamily::UnRar {
                banner_from(&outcome).is_some()
            } else {
                outcome.status == Some(0)
            };
            if ok {
                Some(banner_from(&outcome))
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

pub(crate) fn display_name(family: HandlerFamily, program: &Path, banner: Option<&str>) -> String {
    let file = program
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let banner = banner.unwrap_or("");
    match family {
        HandlerFamily::SevenZip => {
            if banner.contains("NanaZip") {
                format!("NanaZip console ({file})")
            } else if banner.contains("7-Zip") {
                format!("7-Zip ({file})")
            } else {
                format!("7-Zip compatible ({file})")
            }
        }
        HandlerFamily::Tar => {
            if banner.contains("bsdtar") {
                format!("tar.exe bsdtar ({file})")
            } else {
                format!("GNU tar ({file})")
            }
        }
        _ => format!("{} ({file})", family.label()),
    }
}
