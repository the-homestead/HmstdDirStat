//! File-type totals for the Extensions widget: every file under a folder, grouped by extension.
//! Pure data with no GPUI types, so it tests under `cargo test` without a window.

use std::{collections::HashMap, path::Path};

use hmstd_filesystem::{EntryKind, FileTree, NodeId};

use crate::explore::{Basis, bytes_of};

/// One extension group: its name, the bytes its files take under the chosen basis, and how
/// many files it holds. Files with no extension are grouped under the empty name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionTotal {
    pub extension: String,
    pub bytes: u64,
    pub files: u64,
}

/// Totals for every file below `focus`, largest first under `basis`, ties broken by name.
/// Directories are walked, never counted. Extensions compare case-insensitively.
pub fn totals(tree: &FileTree, focus: NodeId, basis: Basis) -> Vec<ExtensionTotal> {
    let mut groups: HashMap<String, (u64, u64)> = HashMap::new();
    let mut pending = vec![focus];
    while let Some(parent) = pending.pop() {
        for child in tree.children(parent) {
            let Some(node) = tree.node(child) else {
                continue;
            };
            if node.kind() == EntryKind::Directory {
                pending.push(child);
                continue;
            }
            let entry = groups.entry(extension_of(node.name())).or_default();
            entry.0 += bytes_of(tree, child, basis);
            entry.1 += 1;
        }
    }
    let mut out: Vec<ExtensionTotal> = groups
        .into_iter()
        .map(|(extension, (bytes, files))| ExtensionTotal {
            extension,
            bytes,
            files,
        })
        .collect();
    out.sort_by(|a, b| {
        b.bytes
            .cmp(&a.bytes)
            .then_with(|| a.extension.cmp(&b.extension))
    });
    out
}

/// The lower-cased extension of a file name, or the empty string when it has none.
fn extension_of(name: &std::ffi::OsStr) -> String {
    Path::new(name)
        .extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hmstd_scanner::{CancelToken, FileSystemScanner, GenericScanner, ScanOptions};

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

    fn write(path: &Path, bytes: usize) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![0u8; bytes]).unwrap();
    }

    #[test]
    fn groups_files_by_extension_across_folders_largest_first() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("a/one.RS"), 300);
        write(&dir.path().join("a/b/two.rs"), 200);
        write(&dir.path().join("three.txt"), 100);
        write(&dir.path().join("README"), 50);
        let tree = scan(dir.path());
        let rows = totals(&tree, NodeId::ROOT, Basis::Logical);
        let names: Vec<&str> = rows.iter().map(|r| r.extension.as_str()).collect();
        assert_eq!(names, ["rs", "txt", ""]);
        assert_eq!(rows[0].bytes, 500);
        assert_eq!(rows[0].files, 2);
        assert_eq!(rows[2].files, 1);
    }

    #[test]
    fn only_files_below_the_focus_are_counted() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("outside.log"), 400);
        write(&dir.path().join("sub/inside.log"), 70);
        let tree = scan(dir.path());
        let sub = tree
            .children(NodeId::ROOT)
            .find(|id| tree.node(*id).is_some_and(|n| n.name() == "sub"))
            .expect("sub folder");
        let rows = totals(&tree, sub, Basis::Logical);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].bytes, 70);
        assert_eq!(rows[0].files, 1);
    }

    #[test]
    fn an_empty_folder_has_no_rows() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("empty")).unwrap();
        let tree = scan(dir.path());
        assert!(totals(&tree, NodeId::ROOT, Basis::Logical).is_empty());
    }
}
