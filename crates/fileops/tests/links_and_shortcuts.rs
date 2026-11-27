//! Symbolic links and `.lnk` shortcuts. Every test writes under a temp
//! folder. The `.lnk` bytes are checked by parsing them back against the
//! Shell Link layout (MS-SHLLINK), so no Windows shell is needed.

use std::fs;
use std::path::{Path, PathBuf};

use timberstat_fileops::{
    Collision, FailureReason, LinkOutcome, ShortcutOutcome, ShortcutSpec, create_shortcut,
    create_symlink, encode_lnk,
};

fn write(path: &Path, data: &[u8]) {
    fs::write(path, data).unwrap();
}

// ---------- symbolic links ----------

#[test]
fn symlink_points_at_the_target() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("target.txt");
    write(&target, b"data");
    let link = tmp.path().join("link.txt");
    match create_symlink(&target, &link, Collision::Rename) {
        Ok(LinkOutcome::Created { path }) => assert_eq!(path, link),
        Err(failure) if failure.reason == FailureReason::AccessDenied => {
            eprintln!("skipped: {}", failure.detail);
            return;
        }
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(fs::read_link(&link).unwrap(), target);
    assert_eq!(fs::read(&link).unwrap(), b"data");
}

#[test]
fn symlink_never_replaces_an_existing_name() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("target.txt");
    write(&target, b"target");
    let link = tmp.path().join("taken.txt");
    write(&link, b"keep me");

    for policy in [Collision::Rename, Collision::Overwrite] {
        let err = create_symlink(&target, &link, policy).unwrap_err();
        assert_eq!(err.reason, FailureReason::Conflict, "{policy:?}");
    }
    assert_eq!(fs::read(&link).unwrap(), b"keep me");
}

#[test]
fn symlink_skip_keeps_the_existing_entry() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("target.txt");
    write(&target, b"target");
    let link = tmp.path().join("taken.txt");
    write(&link, b"keep me");
    let outcome = create_symlink(&target, &link, Collision::Skip).unwrap();
    assert_eq!(
        outcome,
        LinkOutcome::Skipped {
            existing: link.clone()
        }
    );
    assert_eq!(fs::read(&link).unwrap(), b"keep me");
}

#[test]
fn symlink_keep_both_picks_a_free_name() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("target.txt");
    write(&target, b"target");
    let link = tmp.path().join("taken.txt");
    write(&link, b"keep me");
    match create_symlink(&target, &link, Collision::KeepBoth) {
        Ok(LinkOutcome::Created { path }) => {
            assert_eq!(path.file_name().unwrap(), "taken (2).txt");
        }
        Err(failure) if failure.reason == FailureReason::AccessDenied => return,
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(fs::read(&link).unwrap(), b"keep me");
}

#[test]
fn symlink_refuses_a_relative_target() {
    let tmp = tempfile::tempdir().unwrap();
    let err = create_symlink(
        Path::new("relative.txt"),
        &tmp.path().join("link.txt"),
        Collision::Rename,
    )
    .unwrap_err();
    assert_eq!(err.reason, FailureReason::Other);
}

#[test]
fn symlink_refuses_a_missing_target() {
    let tmp = tempfile::tempdir().unwrap();
    let err = create_symlink(
        &tmp.path().join("missing.txt"),
        &tmp.path().join("link.txt"),
        Collision::Rename,
    )
    .unwrap_err();
    assert_eq!(err.reason, FailureReason::NotFound);
}

#[test]
fn symlink_to_a_folder_is_created() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("folder");
    fs::create_dir(&dir).unwrap();
    let link = tmp.path().join("folder-link");
    match create_symlink(&dir, &link, Collision::Rename) {
        Ok(LinkOutcome::Created { .. }) => assert!(fs::metadata(&link).unwrap().is_dir()),
        Err(failure) if failure.reason == FailureReason::AccessDenied => {}
        other => panic!("unexpected {other:?}"),
    }
}

// ---------- shortcuts ----------

/// What the parser reads back from a `.lnk` file.
#[derive(Debug, PartialEq)]
struct Parsed {
    flags: u32,
    attributes: u32,
    file_size: u32,
    show: u32,
    local_base_path: String,
    name: Option<String>,
    working_dir: Option<String>,
}

const LINK_CLSID: [u8; 16] = [
    0x01, 0x14, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46,
];

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

fn utf16_at(b: &[u8], at: usize) -> String {
    let units: Vec<u16> = b[at..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .take_while(|&u| u != 0)
        .collect();
    String::from_utf16(&units).unwrap()
}

fn parse_lnk(b: &[u8]) -> Parsed {
    assert_eq!(u32_at(b, 0), 0x4C, "HeaderSize");
    assert_eq!(&b[4..20], &LINK_CLSID, "LinkCLSID");
    let flags = u32_at(b, 20);
    let attributes = u32_at(b, 24);
    let file_size = u32_at(b, 52);
    let show = u32_at(b, 60);
    let mut pos = 76;

    assert!(flags & 0x2 != 0, "HasLinkInfo must be set");
    let info_size = u32_at(b, pos) as usize;
    let info = &b[pos..pos + info_size];
    assert_eq!(u32_at(info, 4), 0x24, "LinkInfoHeaderSize");
    assert_eq!(u32_at(info, 8) & 0x1, 0x1, "VolumeIDAndLocalBasePath");
    let unicode_off = u32_at(info, 28) as usize;
    let local_base_path = utf16_at(info, unicode_off);
    pos += info_size;

    let read_string = |pos: &mut usize| -> String {
        let count = u16_at(b, *pos) as usize;
        let start = *pos + 2;
        let text = utf16_at(&b[..start + count * 2], start);
        *pos = start + count * 2;
        text
    };
    let name = (flags & 0x4 != 0).then(|| read_string(&mut pos));
    let working_dir = (flags & 0x10 != 0).then(|| read_string(&mut pos));

    assert_eq!(pos + 4, b.len(), "only the terminal block may follow");
    assert_eq!(u32_at(b, pos), 0, "TerminalID");
    Parsed {
        flags,
        attributes,
        file_size,
        show,
        local_base_path,
        name,
        working_dir,
    }
}

#[test]
fn encoded_lnk_round_trips_through_the_layout() {
    let spec = ShortcutSpec {
        target: PathBuf::from("C:\\Games\\Tool.exe"),
        is_dir: false,
        size: 4096,
        description: Some("Tool".to_owned()),
        working_dir: Some(PathBuf::from("C:\\Games")),
    };
    let bytes = encode_lnk(&spec, 0x01D9_0000_0000_0000).unwrap();
    let parsed = parse_lnk(&bytes);
    assert_eq!(parsed.local_base_path, "C:\\Games\\Tool.exe");
    assert_eq!(parsed.name.as_deref(), Some("Tool"));
    assert_eq!(parsed.working_dir.as_deref(), Some("C:\\Games"));
    assert_eq!(parsed.file_size, 4096);
    assert_eq!(parsed.attributes, 0x80);
    assert_eq!(parsed.show, 1);
    assert_eq!(parsed.flags & 0x80, 0x80, "IsUnicode");
}

#[test]
fn folder_target_sets_the_directory_attribute_and_no_size() {
    let spec = ShortcutSpec {
        target: PathBuf::from("C:\\Data"),
        is_dir: true,
        size: 0,
        description: None,
        working_dir: None,
    };
    let parsed = parse_lnk(&encode_lnk(&spec, 0).unwrap());
    assert_eq!(parsed.attributes, 0x10);
    assert_eq!(parsed.file_size, 0);
    assert_eq!(parsed.name, None);
    assert_eq!(parsed.working_dir, None);
    assert_eq!(parsed.flags & 0x14, 0, "no name, no working folder");
}

#[test]
fn non_ascii_target_survives_the_unicode_path() {
    let spec = ShortcutSpec {
        target: PathBuf::from("C:\\Users\\naïve 日本\\report.pdf"),
        is_dir: false,
        size: 1,
        description: Some("報告".to_owned()),
        working_dir: None,
    };
    let parsed = parse_lnk(&encode_lnk(&spec, 0).unwrap());
    assert_eq!(parsed.local_base_path, "C:\\Users\\naïve 日本\\report.pdf");
    assert_eq!(parsed.name.as_deref(), Some("報告"));
}

#[test]
fn overlong_description_is_refused() {
    let spec = ShortcutSpec {
        target: PathBuf::from("C:\\a.txt"),
        is_dir: false,
        size: 0,
        description: Some("x".repeat(1025)),
        working_dir: None,
    };
    assert!(encode_lnk(&spec, 0).is_err());
}

#[test]
fn shortcut_file_is_written_and_parses_back() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("doc.txt");
    write(&target, b"hello");
    let link = tmp.path().join("Doc.LNK");
    let outcome = create_shortcut(&target, &link, Collision::Rename, Some("Doc"), None).unwrap();
    assert_eq!(outcome, ShortcutOutcome::Created { path: link.clone() });
    let parsed = parse_lnk(&fs::read(&link).unwrap());
    assert_eq!(parsed.local_base_path, target.to_string_lossy());
    assert_eq!(parsed.file_size, 5);
    assert_eq!(parsed.name.as_deref(), Some("Doc"));
}

#[test]
fn shortcut_needs_an_lnk_name_and_never_overwrites() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("doc.txt");
    write(&target, b"hello");

    let wrong_ext = create_shortcut(
        &target,
        &tmp.path().join("doc.url"),
        Collision::Rename,
        None,
        None,
    );
    assert_eq!(wrong_ext.unwrap_err().reason, FailureReason::InvalidName);

    let link = tmp.path().join("taken.lnk");
    write(&link, b"precious");
    let err = create_shortcut(&target, &link, Collision::Overwrite, None, None).unwrap_err();
    assert_eq!(err.reason, FailureReason::Conflict);
    let err = create_shortcut(&target, &link, Collision::Rename, None, None).unwrap_err();
    assert_eq!(err.reason, FailureReason::Conflict);
    assert_eq!(fs::read(&link).unwrap(), b"precious");

    let skipped = create_shortcut(&target, &link, Collision::Skip, None, None).unwrap();
    assert_eq!(
        skipped,
        ShortcutOutcome::Skipped {
            existing: link.clone()
        }
    );

    let both = create_shortcut(&target, &link, Collision::KeepBoth, None, None).unwrap();
    assert_eq!(
        both,
        ShortcutOutcome::Created {
            path: tmp.path().join("taken (2).lnk"),
        }
    );
}

#[test]
fn shortcut_refuses_a_missing_target() {
    let tmp = tempfile::tempdir().unwrap();
    let err = create_shortcut(
        &tmp.path().join("gone.txt"),
        &tmp.path().join("gone.lnk"),
        Collision::Rename,
        None,
        None,
    )
    .unwrap_err();
    assert_eq!(err.reason, FailureReason::NotFound);
    assert!(!tmp.path().join("gone.lnk").exists());
}
