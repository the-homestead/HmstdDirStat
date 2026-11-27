//! The Files tab's rules, with no GPUI types so they can be tested: the sort order, the hidden
//! file rule, the modification-time text, the first folders to open, and the name paths that
//! carry a view across a rescan.

use std::cmp::Ordering;
use std::collections::HashSet;
use std::ffi::{OsStr, OsString};

use timberstat_filesystem::{EntryKind, FileTree, NodeId};

use crate::explore::{self, Basis, Child, bytes_of};
use crate::extensions::extension_of;

/// A column the Files tab can sort by. Each header cell names one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Name,
    Percent,
    Size,
    OnDisk,
    Items,
    Modified,
    Extension,
}

/// The Files tab's sort: one column and a direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sort {
    pub column: Column,
    pub descending: bool,
}

impl Default for Sort {
    /// Largest first, which is how the tab sorted before the columns were clickable.
    fn default() -> Self {
        Self {
            column: Column::Size,
            descending: true,
        }
    }
}

impl Sort {
    /// The order after a click on `column`. The same column flips its direction. Another column
    /// starts in its natural direction: sizes, counts and dates largest or newest first, names
    /// and extensions A to Z.
    pub fn clicked(self, column: Column) -> Sort {
        if self.column == column {
            Sort {
                column,
                descending: !self.descending,
            }
        } else {
            Sort {
                column,
                descending: !matches!(column, Column::Name | Column::Extension),
            }
        }
    }
}

/// Orders two siblings under `sort`. Ties fall back to the name, then the node index, so rows
/// keep their place from one build to the next.
pub fn compare(tree: &FileTree, a: &Child, b: &Child, sort: Sort) -> Ordering {
    let natural = match sort.column {
        Column::Name => lower(&a.name).cmp(&lower(&b.name)),
        Column::Percent | Column::Size => a.bytes.cmp(&b.bytes),
        Column::OnDisk => {
            bytes_of(tree, a.node, Basis::Allocated).cmp(&bytes_of(tree, b.node, Basis::Allocated))
        }
        Column::Items => items_of(tree, a.node).cmp(&items_of(tree, b.node)),
        Column::Modified => modified_of(tree, a.node).cmp(&modified_of(tree, b.node)),
        Column::Extension => {
            extension_of(OsStr::new(&a.name)).cmp(&extension_of(OsStr::new(&b.name)))
        }
    };
    let ordered = if sort.descending {
        natural.reverse()
    } else {
        natural
    };
    ordered
        .then_with(|| lower(&a.name).cmp(&lower(&b.name)))
        .then_with(|| a.node.index().cmp(&b.node.index()))
}

fn lower(name: &str) -> String {
    name.to_lowercase()
}

/// Files and folders below a folder. Zero for files.
fn items_of(tree: &FileTree, node: NodeId) -> u64 {
    match tree.node(node) {
        Some(entry) if entry.kind() == EntryKind::Directory => {
            let totals = entry.totals();
            totals.files + totals.directories
        }
        _ => 0,
    }
}

/// The modification time, or `None` when the scan did not record one.
fn modified_of(tree: &FileTree, node: NodeId) -> Option<i64> {
    tree.node(node)
        .and_then(|entry| entry.metadata().modified_unix_secs)
}

/// True for the entries the Hidden toggle hides: the Windows hidden attribute, or a name that
/// starts with a dot. This is the same rule as `attr:hidden` in search.
pub fn is_hidden(tree: &FileTree, node: NodeId) -> bool {
    tree.node(node).is_some_and(|entry| {
        entry.metadata().is_hidden_attribute() || entry.name().to_string_lossy().starts_with('.')
    })
}

/// The kind of file an icon is drawn for, from its extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Archive,
    Image,
    Audio,
    Video,
    Code,
    Document,
    Sheet,
    Other,
}

/// The icon kind of a file name, by its lower-cased extension. Anything not listed is `Other`.
pub fn file_kind(name: &OsStr) -> FileKind {
    match extension_of(name).as_str() {
        "zip" | "rar" | "7z" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "iso" | "cab" => {
            FileKind::Archive
        }
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "svg" | "ico" | "tif" | "tiff"
        | "psd" | "heic" | "avif" => FileKind::Image,
        "mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac" | "wma" | "opus" => FileKind::Audio,
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "webm" | "m4v" | "flv" => FileKind::Video,
        "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "c" | "h" | "cc" | "cpp" | "hpp" | "cs"
        | "java" | "kt" | "swift" | "go" | "rb" | "php" | "lua" | "sh" | "ps1" | "bat" | "html"
        | "css" | "json" | "toml" | "yaml" | "yml" | "xml" => FileKind::Code,
        "txt" | "md" | "pdf" | "doc" | "docx" | "rtf" | "odt" | "log" | "epub" => {
            FileKind::Document
        }
        "xls" | "xlsx" | "ods" | "csv" | "tsv" => FileKind::Sheet,
        _ => FileKind::Other,
    }
}

/// `YYYY-MM-DD HH:MM` for a Unix time in seconds, in UTC. Dates before 1970 are negative.
pub fn format_modified(unix_secs: i64) -> String {
    let days = unix_secs.div_euclid(86_400);
    let second_of_day = unix_secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        second_of_day / 3_600,
        second_of_day % 3_600 / 60
    )
}

/// The Gregorian date `days` after 1970-01-01, as (year, month, day). Howard Hinnant's
/// `civil_from_days`, which is exact for the whole range of `i64` day counts used here.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month as u32, day as u32)
}

/// The names from the root's child down to `node`. The root itself is not part of the chain,
/// so the chain of the root is empty. Names identify a node across a rescan, when node indexes
/// change.
pub fn name_chain(tree: &FileTree, node: NodeId) -> Vec<OsString> {
    let mut chain = Vec::new();
    let mut current = node;
    while current != NodeId::ROOT {
        let Some(entry) = tree.node(current) else {
            break;
        };
        chain.push(entry.name().to_os_string());
        match entry.parent() {
            Some(parent) => current = parent,
            None => break,
        }
    }
    chain.reverse();
    chain
}

/// The node a name chain leads to in `tree`, or `None` when a name is no longer there.
pub fn resolve_chain(tree: &FileTree, chain: &[OsString]) -> Option<NodeId> {
    chain.iter().try_fold(NodeId::ROOT, |parent, name| {
        tree.children(parent).find(|&child| {
            tree.node(child)
                .is_some_and(|entry| entry.name() == name.as_os_str())
        })
    })
}

/// The folders open when a tree first loads: the largest `limit` folders in each of the top
/// `levels` levels. These are the rows the tab shows at first, so the view matches the old one.
pub fn initial_expanded(tree: &FileTree, limit: usize, levels: u32) -> HashSet<NodeId> {
    let mut open = HashSet::new();
    let mut parents = vec![NodeId::ROOT];
    for _ in 0..levels {
        parents = parents
            .iter()
            .flat_map(|&parent| {
                explore::sorted_children(tree, parent, Sort::default())
                    .into_iter()
                    .take(limit)
            })
            .filter(|child| child.is_dir)
            .map(|child| child.node)
            .collect();
        open.extend(parents.iter().copied());
    }
    open
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use timberstat_scanner::{CancelToken, FileSystemScanner, GenericScanner, ScanOptions};

    fn write(path: &Path, bytes: usize) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![0u8; bytes]).unwrap();
    }

    fn scan(root: &Path) -> FileTree {
        GenericScanner
            .scan(
                root,
                &ScanOptions::default(),
                &CancelToken::new(),
                &mut |_| {},
            )
            .unwrap()
            .tree
    }

    /// Root children: `big` (a folder of 5000 bytes), `mid.bin` 900, `small.bin` 100,
    /// `tiny.bin` 50, `Readme` 70 (no extension) and `.hidden` 30 (a dot file).
    fn sample() -> (tempfile::TempDir, FileTree) {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("big/inner/x.bin"), 5000);
        write(&dir.path().join("mid.bin"), 900);
        write(&dir.path().join("small.bin"), 100);
        write(&dir.path().join("tiny.bin"), 50);
        write(&dir.path().join("Readme"), 70);
        write(&dir.path().join(".hidden"), 30);
        let tree = scan(dir.path());
        (dir, tree)
    }

    fn names(tree: &FileTree, sort: Sort) -> Vec<String> {
        explore::sorted_children(tree, NodeId::ROOT, sort)
            .into_iter()
            .map(|child| child.name)
            .collect()
    }

    #[test]
    fn default_sort_is_largest_first_as_before() {
        let (_dir, tree) = sample();
        assert_eq!(
            names(&tree, Sort::default()),
            [
                "big",
                "mid.bin",
                "small.bin",
                "Readme",
                "tiny.bin",
                ".hidden"
            ]
        );
    }

    #[test]
    fn name_sort_ignores_case_in_both_directions() {
        let (_dir, tree) = sample();
        let asc = Sort {
            column: Column::Name,
            descending: false,
        };
        assert_eq!(
            names(&tree, asc),
            [
                ".hidden",
                "big",
                "mid.bin",
                "Readme",
                "small.bin",
                "tiny.bin"
            ]
        );
        assert_eq!(
            names(&tree, asc.clicked(Column::Name)),
            [
                ".hidden",
                "big",
                "mid.bin",
                "Readme",
                "small.bin",
                "tiny.bin"
            ]
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
        );
    }

    #[test]
    fn extension_sort_groups_by_extension_then_name() {
        let (_dir, tree) = sample();
        let asc = Sort {
            column: Column::Extension,
            descending: false,
        };
        // No extension first (the dot file and the folder and Readme by name), then `.bin`.
        assert_eq!(
            names(&tree, asc),
            [
                ".hidden",
                "big",
                "Readme",
                "mid.bin",
                "small.bin",
                "tiny.bin"
            ]
        );
    }

    #[test]
    fn clicking_a_header_flips_or_takes_the_natural_direction() {
        let size = Sort::default();
        assert!(!size.clicked(Column::Size).descending);
        assert!(size.clicked(Column::Size).clicked(Column::Size).descending);
        assert!(!size.clicked(Column::Name).descending);
        assert!(size.clicked(Column::Modified).descending);
        assert!(!size.clicked(Column::Extension).descending);
    }

    #[test]
    fn hidden_rule_is_a_dot_name_here() {
        let (_dir, tree) = sample();
        let hidden = explore::sorted_children(&tree, NodeId::ROOT, Sort::default())
            .into_iter()
            .filter(|child| is_hidden(&tree, child.node))
            .map(|child| child.name)
            .collect::<Vec<_>>();
        assert_eq!(hidden, [".hidden"]);
    }

    #[test]
    fn file_kind_follows_the_extension_only() {
        let kind = |name: &str| file_kind(OsStr::new(name));
        assert_eq!(kind("backup.TAR.GZ"), FileKind::Archive);
        assert_eq!(kind("photo.JPG"), FileKind::Image);
        assert_eq!(kind("song.flac"), FileKind::Audio);
        assert_eq!(kind("clip.mkv"), FileKind::Video);
        assert_eq!(kind("main.rs"), FileKind::Code);
        assert_eq!(kind("notes.md"), FileKind::Document);
        assert_eq!(kind("sheet.xlsx"), FileKind::Sheet);
        assert_eq!(kind("Makefile"), FileKind::Other);
        assert_eq!(
            kind(".bashrc"),
            FileKind::Other,
            "a dot file has no extension"
        );
    }

    #[test]
    fn formats_unix_seconds_as_utc_minutes() {
        assert_eq!(format_modified(0), "1970-01-01 00:00");
        assert_eq!(format_modified(-1), "1969-12-31 23:59");
        assert_eq!(format_modified(951_782_400), "2000-02-29 00:00");
        assert_eq!(format_modified(1_700_000_000), "2023-11-14 22:13");
    }

    #[test]
    fn name_chain_round_trips_to_the_same_node() {
        let (_dir, tree) = sample();
        let inner = tree
            .children(NodeId::ROOT)
            .find(|&id| tree.node(id).unwrap().name() == "big")
            .and_then(|big| tree.children(big).next())
            .unwrap();
        let chain = name_chain(&tree, inner);
        assert_eq!(chain, vec![OsString::from("big"), OsString::from("inner")]);
        assert_eq!(resolve_chain(&tree, &chain), Some(inner));
        assert!(name_chain(&tree, NodeId::ROOT).is_empty());
        assert_eq!(resolve_chain(&tree, &[]), Some(NodeId::ROOT));
        assert_eq!(
            resolve_chain(&tree, &[OsString::from("gone")]),
            None,
            "a name that is no longer there does not resolve"
        );
    }

    #[test]
    fn initial_open_folders_are_the_top_two_levels() {
        let (_dir, tree) = sample();
        let open = initial_expanded(&tree, 40, 2);
        let big = resolve_chain(&tree, &[OsString::from("big")]).unwrap();
        let inner =
            resolve_chain(&tree, &[OsString::from("big"), OsString::from("inner")]).unwrap();
        assert!(open.contains(&big));
        assert!(open.contains(&inner));
        assert_eq!(open.len(), 2, "files are never opened");
    }
}
