//! Builds a small NTFS volume image in memory and reads it back through the
//! public API. Covers fixups, resident and non-resident data, names from several
//! namespaces, a hard link, a deleted record, an orphan, and a sparse-free MFT.

use std::io::Cursor;

use timberstat_ntfs::{Catalog, parse_boot_sector, read_records};

const CLUSTER: usize = 4096;
const RECORD: usize = 1024;
const MFT_LCN: usize = 4;

fn attr_resident(attr_type: u32, value: &[u8]) -> Vec<u8> {
    let length = (24 + value.len() + 7) & !7;
    let mut attr = vec![0u8; length];
    attr[0..4].copy_from_slice(&attr_type.to_le_bytes());
    attr[4..8].copy_from_slice(&(length as u32).to_le_bytes());
    attr[16..20].copy_from_slice(&(value.len() as u32).to_le_bytes());
    attr[20..22].copy_from_slice(&24u16.to_le_bytes());
    attr[24..24 + value.len()].copy_from_slice(value);
    attr
}

fn attr_nonresident_data(logical: u64, allocated: u64, runs: &[u8]) -> Vec<u8> {
    let length = (64 + runs.len() + 7) & !7;
    let mut attr = vec![0u8; length];
    attr[0..4].copy_from_slice(&0x80u32.to_le_bytes());
    attr[4..8].copy_from_slice(&(length as u32).to_le_bytes());
    attr[8] = 1;
    attr[32..34].copy_from_slice(&64u16.to_le_bytes());
    attr[40..48].copy_from_slice(&allocated.to_le_bytes());
    attr[48..56].copy_from_slice(&logical.to_le_bytes());
    attr[56..64].copy_from_slice(&logical.to_le_bytes());
    attr[64..64 + runs.len()].copy_from_slice(runs);
    attr
}

fn file_name_value(parent: u64, namespace: u8, name: &str) -> Vec<u8> {
    let units: Vec<u16> = name.encode_utf16().collect();
    let mut value = vec![0u8; 0x42 + units.len() * 2];
    value[0..8].copy_from_slice(&parent.to_le_bytes());
    value[0x40] = units.len() as u8;
    value[0x41] = namespace;
    for (index, unit) in units.iter().enumerate() {
        value[0x42 + index * 2..0x44 + index * 2].copy_from_slice(&unit.to_le_bytes());
    }
    value
}

fn attr_name(parent: u64, namespace: u8, name: &str) -> Vec<u8> {
    attr_resident(0x30, &file_name_value(parent, namespace, name))
}

/// One 1 KiB record with the update sequence applied, as the disk stores it.
fn record(flags: u16, base: u64, attrs: &[Vec<u8>]) -> Vec<u8> {
    let mut buf = vec![0u8; RECORD];
    buf[0..4].copy_from_slice(b"FILE");
    buf[4..6].copy_from_slice(&0x30u16.to_le_bytes()); // update sequence array offset
    buf[6..8].copy_from_slice(&3u16.to_le_bytes()); // USN plus one entry per sector
    buf[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
    buf[0x16..0x18].copy_from_slice(&flags.to_le_bytes());
    buf[0x20..0x28].copy_from_slice(&base.to_le_bytes());

    let mut pos = 0x38;
    for attr in attrs {
        buf[pos..pos + attr.len()].copy_from_slice(attr);
        pos += attr.len();
    }
    buf[pos..pos + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());

    // Fixups: keep the real tail bytes in the array, write the USN over them.
    let usn = [0x01u8, 0x00];
    buf[0x30..0x32].copy_from_slice(&usn);
    for (index, sector_end) in [510usize, 1022].iter().enumerate() {
        let original = [buf[*sector_end], buf[*sector_end + 1]];
        buf[0x32 + index * 2..0x34 + index * 2].copy_from_slice(&original);
        buf[*sector_end..*sector_end + 2].copy_from_slice(&usn);
    }
    buf
}

fn empty_slot() -> Vec<u8> {
    vec![0u8; RECORD]
}

fn boot_sector() -> Vec<u8> {
    let mut sector = vec![0u8; 512];
    sector[3..11].copy_from_slice(b"NTFS    ");
    sector[0x0B..0x0D].copy_from_slice(&512u16.to_le_bytes());
    sector[0x0D] = 8;
    sector[0x30..0x38].copy_from_slice(&(MFT_LCN as u64).to_le_bytes());
    sector[0x40] = (-10i8) as u8;
    sector
}

/// Volume layout: boot sector, then the MFT at cluster 4 holding 16 records.
fn volume() -> Vec<u8> {
    let mut image = vec![0u8; MFT_LCN * CLUSTER];
    image[..512].copy_from_slice(&boot_sector());

    let mft_runs = [0x11, 0x04, 0x04, 0x00]; // 4 clusters at LCN 4
    let slots: Vec<Vec<u8>> = vec![
        // 0: $MFT itself, 16 records long.
        record(
            0x01,
            0,
            &[
                attr_name(5, 1, "$MFT"),
                attr_nonresident_data(16 * RECORD as u64, 16 * RECORD as u64, &mft_runs),
            ],
        ),
        empty_slot(),
        empty_slot(),
        empty_slot(),
        empty_slot(),
        // 5: root directory.
        record(0x03, 0, &[attr_name(5, 3, ".")]),
        // 6: docs/ directory under the root.
        record(0x03, 0, &[attr_name(5, 1, "docs")]),
        // 7: a.txt, small enough to be resident.
        record(
            0x01,
            0,
            &[attr_name(5, 1, "a.txt"), attr_resident(0x80, b"0123456789")],
        ),
        // 8: docs/b.bin, 100000 bytes in a 102400-byte allocation.
        record(
            0x01,
            0,
            &[
                attr_name(6, 1, "b.bin"),
                attr_nonresident_data(100_000, 102_400, &[0x11, 0x19, 0x10, 0x00]),
            ],
        ),
        // 9: deleted record, must be ignored.
        record(
            0x00,
            0,
            &[attr_name(5, 1, "gone.txt"), attr_resident(0x80, b"xx")],
        ),
        // 10: in use, but its parent record 99 does not exist.
        record(
            0x01,
            0,
            &[attr_name(99, 1, "lost.txt"), attr_resident(0x80, b"yy")],
        ),
        // 11: one file with a Win32 name at the root and a DOS name under docs.
        record(
            0x01,
            0,
            &[
                attr_name(6, 2, "HARD~1.TXT"),
                attr_name(5, 1, "hard.txt"),
                attr_resident(0x80, b"zzzz"),
            ],
        ),
    ];
    let mut body: Vec<u8> = slots.concat();
    body.extend(std::iter::repeat_n(0u8, 16 * RECORD - body.len()));
    image.extend(body);
    image
}

fn build_tree() -> (
    timberstat_filesystem::FileTree,
    timberstat_ntfs::BuildStats,
    timberstat_ntfs::ReadStats,
) {
    let image = volume();
    let mut cursor = Cursor::new(image);
    let mut sector = [0u8; 512];
    std::io::Read::read_exact(&mut cursor, &mut sector).unwrap();
    let boot = parse_boot_sector(&sector).unwrap();

    let mut catalog = Catalog::new();
    let stats = read_records(&mut cursor, boot, |number, record| {
        catalog.insert(number, record);
    })
    .unwrap();
    let (tree, build) = catalog.into_tree("C:").unwrap();
    (tree, build, stats)
}

fn child_named(
    tree: &timberstat_filesystem::FileTree,
    parent: timberstat_filesystem::NodeId,
    name: &str,
) -> Option<timberstat_filesystem::NodeId> {
    tree.children(parent).find(|id| {
        tree.node(*id)
            .is_some_and(|node| node.name() == std::ffi::OsStr::new(name))
    })
}

#[test]
fn synthetic_volume_builds_the_expected_tree() {
    let (tree, build, stats) = build_tree();

    // Slots 0..=15 are read; 4 of the 12 used/empty slots are plain empty records.
    assert_eq!(stats.bad_records, 0);
    assert_eq!(build.orphans, 1, "lost.txt has no parent");

    let root = timberstat_filesystem::NodeId::ROOT;
    assert!(child_named(&tree, root, "a.txt").is_some());
    assert!(child_named(&tree, root, "docs").is_some());
    assert!(
        child_named(&tree, root, "hard.txt").is_some(),
        "Win32 name wins"
    );
    assert!(
        child_named(&tree, root, "gone.txt").is_none(),
        "deleted record skipped"
    );
    assert!(
        child_named(&tree, root, "lost.txt").is_none(),
        "orphan not placed"
    );

    let docs = child_named(&tree, root, "docs").unwrap();
    let b = child_named(&tree, docs, "b.bin").unwrap();
    assert_eq!(tree.node(b).unwrap().logical_size(), 100_000);
    assert_eq!(tree.node(b).unwrap().physical_size(), Some(102_400));
    assert!(
        child_named(&tree, docs, "HARD~1.TXT").is_none(),
        "DOS name not used when Win32 exists"
    );

    let totals = tree.node(root).unwrap().totals();
    // a.txt (10) + b.bin (100000) + hard.txt (4) + $MFT itself (16384): the MFT is a real file.
    assert_eq!(totals.logical, 10 + 100_000 + 4 + 16_384);
    assert!(child_named(&tree, root, "$MFT").is_some());
    assert!(totals.complete);
}

#[test]
fn record_size_and_mft_location_come_from_the_boot_sector() {
    let image = volume();
    let mut cursor = Cursor::new(image);
    let mut sector = [0u8; 512];
    std::io::Read::read_exact(&mut cursor, &mut sector).unwrap();
    let boot = parse_boot_sector(&sector).unwrap();
    assert_eq!(boot.record_size, 1024);
    assert_eq!(boot.mft_lcn, MFT_LCN as u64);
}
