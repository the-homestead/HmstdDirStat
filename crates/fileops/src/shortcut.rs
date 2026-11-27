//! Windows shortcut (`.lnk`) files, written in the Shell Link binary format
//! (MS-SHLLINK) with no `unsafe` and no COM.
//!
//! The file carries the target as a LinkInfo local base path (ANSI and
//! UTF-16), plus an optional description and working folder. It has no link
//! target ID list. The format makes that list optional, and the shell
//! resolves the target from the LinkInfo.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::link::{Resolved, check_target, resolve_link_name};
use crate::model::{Failure, FailureReason};
use crate::transfer_plan::Collision;

/// Result of a shortcut creation that did not fail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShortcutOutcome {
    /// The `.lnk` file now exists at this path.
    Created { path: PathBuf },
    /// The name was taken and the policy was skip. Nothing changed.
    Skipped { existing: PathBuf },
}

/// Longest description accepted, in characters.
pub const MAX_DESCRIPTION_CHARS: usize = 1024;

const HEADER_SIZE: u32 = 0x4C;
const LINK_CLSID: [u8; 16] = [
    0x01, 0x14, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46,
];
const HAS_LINK_INFO: u32 = 0x0000_0002;
const HAS_NAME: u32 = 0x0000_0004;
const HAS_WORKING_DIR: u32 = 0x0000_0010;
const IS_UNICODE: u32 = 0x0000_0080;
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;
const FILE_ATTRIBUTE_NORMAL: u32 = 0x0000_0080;
const SHOW_NORMAL: u32 = 1;
const LINK_INFO_HEADER_SIZE: u32 = 0x24;
const VOLUME_ID_AND_LOCAL_BASE_PATH: u32 = 0x0000_0001;
const DRIVE_FIXED: u32 = 3;
const VOLUME_ID_HEADER: u32 = 0x10;
/// Seconds from 1601-01-01 (FILETIME epoch) to 1970-01-01 (Unix epoch).
const FILETIME_UNIX_OFFSET_SECS: u64 = 11_644_473_600;

/// What a shortcut points at and how it is described.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutSpec {
    /// Absolute path of the file or folder the shortcut opens.
    pub target: PathBuf,
    pub is_dir: bool,
    /// Size of the target in bytes. Ignored for folders.
    pub size: u64,
    /// Name shown in the shortcut's properties.
    pub description: Option<String>,
    /// Folder the target starts in. `None` leaves it unset.
    pub working_dir: Option<PathBuf>,
}

/// Creates a `.lnk` shortcut at `link` that points at `target`.
///
/// `link` must end in `.lnk`. The target must exist and be absolute. The
/// collision policy works as for links: `Skip` and `KeepBoth` choose another
/// outcome, `Overwrite` is refused, and an existing name is never replaced.
/// The file is written with an exclusive create. A failed write removes the
/// partial file this call made.
pub fn create_shortcut(
    target: &Path,
    link: &Path,
    collision: Collision,
    description: Option<&str>,
    working_dir: Option<&Path>,
) -> Result<ShortcutOutcome, Failure> {
    let has_lnk_ext = link
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lnk"));
    if !has_lnk_ext {
        return Err(Failure::new(
            link,
            FailureReason::InvalidName,
            "shortcut files end in .lnk",
        ));
    }
    let meta = check_target(target)?;
    let spec = ShortcutSpec {
        target: target.to_path_buf(),
        is_dir: meta.is_dir(),
        size: meta.len(),
        description: description.map(str::to_owned),
        working_dir: working_dir.map(Path::to_path_buf),
    };
    let bytes = encode_lnk(&spec, filetime_now())
        .map_err(|detail| Failure::new(link, FailureReason::InvalidName, detail))?;

    let final_path = match resolve_link_name(link, collision, false)? {
        Resolved::Free(path) => path,
        Resolved::Skip(existing) => return Ok(ShortcutOutcome::Skipped { existing }),
    };
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&final_path)
        .map_err(|error| Failure::from_io(&final_path, &error))?;
    let written = file.write_all(&bytes).and_then(|()| file.flush());
    drop(file);
    if let Err(error) = written {
        let _ = fs::remove_file(&final_path);
        return Err(Failure::from_io(&final_path, &error));
    }
    Ok(ShortcutOutcome::Created { path: final_path })
}

/// Encodes a shortcut file. `now` is the FILETIME (100 ns ticks since 1601)
/// written as creation, access and write time. Pure: touches no disk.
pub fn encode_lnk(spec: &ShortcutSpec, now: u64) -> Result<Vec<u8>, String> {
    let target = spec.target.to_string_lossy().into_owned();
    if target.encode_utf16().count() >= 32_767 {
        return Err("the target path is too long for a shortcut".to_owned());
    }
    let description = spec.description.as_deref().unwrap_or("");
    if description.chars().count() > MAX_DESCRIPTION_CHARS {
        return Err("the description is too long for a shortcut".to_owned());
    }
    let working = spec
        .working_dir
        .as_ref()
        .map(|dir| dir.to_string_lossy().into_owned());
    if let Some(dir) = &working
        && dir.encode_utf16().count() >= 32_767
    {
        return Err("the working folder path is too long for a shortcut".to_owned());
    }

    let mut flags = HAS_LINK_INFO | IS_UNICODE;
    if !description.is_empty() {
        flags |= HAS_NAME;
    }
    if working.is_some() {
        flags |= HAS_WORKING_DIR;
    }

    let mut out = Vec::with_capacity(256);
    out.extend_from_slice(&HEADER_SIZE.to_le_bytes());
    out.extend_from_slice(&LINK_CLSID);
    out.extend_from_slice(&flags.to_le_bytes());
    let attributes = if spec.is_dir {
        FILE_ATTRIBUTE_DIRECTORY
    } else {
        FILE_ATTRIBUTE_NORMAL
    };
    out.extend_from_slice(&attributes.to_le_bytes());
    for _ in 0..3 {
        out.extend_from_slice(&now.to_le_bytes());
    }
    let size = if spec.is_dir { 0 } else { spec.size as u32 };
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes()); // IconIndex
    out.extend_from_slice(&SHOW_NORMAL.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // HotKey
    out.extend_from_slice(&0u16.to_le_bytes()); // Reserved1
    out.extend_from_slice(&0u32.to_le_bytes()); // Reserved2
    out.extend_from_slice(&0u32.to_le_bytes()); // Reserved3

    out.extend_from_slice(&link_info(&target));

    if !description.is_empty() {
        push_string_data(&mut out, description);
    }
    if let Some(dir) = &working {
        push_string_data(&mut out, dir);
    }
    out.extend_from_slice(&0u32.to_le_bytes()); // terminal block
    Ok(out)
}

/// LinkInfo for a local target: a fixed-type volume ID and the local base
/// path in ANSI and UTF-16 form, with an empty common path suffix.
fn link_info(target: &str) -> Vec<u8> {
    let volume_id = {
        let mut v = Vec::with_capacity(VOLUME_ID_HEADER as usize + 1);
        v.extend_from_slice(&(VOLUME_ID_HEADER + 1).to_le_bytes());
        v.extend_from_slice(&DRIVE_FIXED.to_le_bytes());
        v.extend_from_slice(&0u32.to_le_bytes()); // drive serial
        v.extend_from_slice(&VOLUME_ID_HEADER.to_le_bytes()); // label offset
        v.push(0); // empty label
        v
    };
    let ansi_path: Vec<u8> = target
        .chars()
        .map(|c| if c.is_ascii() { c as u8 } else { b'?' })
        .chain(std::iter::once(0))
        .collect();
    let unicode_path: Vec<u8> = target
        .encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect();
    let empty_suffix_ansi = [0u8];
    let empty_suffix_unicode = [0u8, 0u8];

    let volume_off = LINK_INFO_HEADER_SIZE;
    let ansi_off = volume_off + volume_id.len() as u32;
    let suffix_off = ansi_off + ansi_path.len() as u32;
    let unicode_off = suffix_off + empty_suffix_ansi.len() as u32;
    let unicode_suffix_off = unicode_off + unicode_path.len() as u32;
    let total = unicode_suffix_off + empty_suffix_unicode.len() as u32;

    let mut out = Vec::with_capacity(total as usize);
    out.extend_from_slice(&total.to_le_bytes());
    out.extend_from_slice(&LINK_INFO_HEADER_SIZE.to_le_bytes());
    out.extend_from_slice(&VOLUME_ID_AND_LOCAL_BASE_PATH.to_le_bytes());
    out.extend_from_slice(&volume_off.to_le_bytes());
    out.extend_from_slice(&ansi_off.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // CommonNetworkRelativeLink: none
    out.extend_from_slice(&suffix_off.to_le_bytes());
    out.extend_from_slice(&unicode_off.to_le_bytes());
    out.extend_from_slice(&unicode_suffix_off.to_le_bytes());
    out.extend_from_slice(&volume_id);
    out.extend_from_slice(&ansi_path);
    out.extend_from_slice(&empty_suffix_ansi);
    out.extend_from_slice(&unicode_path);
    out.extend_from_slice(&empty_suffix_unicode);
    out
}

/// A StringData entry: a UTF-16 count in characters, then the characters.
fn push_string_data(out: &mut Vec<u8>, text: &str) {
    let units: Vec<u16> = text.encode_utf16().collect();
    out.extend_from_slice(&(units.len() as u16).to_le_bytes());
    for unit in units {
        out.extend_from_slice(&unit.to_le_bytes());
    }
}

/// The current time as a FILETIME value.
pub(crate) fn filetime_now() -> u64 {
    let since_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    (since_unix.as_secs() + FILETIME_UNIX_OFFSET_SECS) * 10_000_000
        + u64::from(since_unix.subsec_nanos() / 100)
}
