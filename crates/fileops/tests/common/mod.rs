//! Shared fixtures: build a FileTree from a real temp directory, as a scanner would.
#![allow(dead_code, reason = "each test binary uses a different subset")]

use std::fs;
use std::path::{Path, PathBuf};

use timberstat_fileops::tree_base;
use timberstat_filesystem::{EntryKind, FileTree, Metadata, NewEntry, NodeId};

/// Writes a file, creating parent folders.
pub fn write(path: impl AsRef<Path>, bytes: &[u8]) {
    let path = path.as_ref();
    fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    fs::write(path, bytes).expect("write fixture");
}

/// Builds a tree from `root` the way the scanner does: root named by its file
/// name, sizes from disk metadata, links recorded as symlinks and not followed.
pub fn scan(root: &Path) -> FileTree {
    let name = root.file_name().expect("root name").to_os_string();
    let mut root_entry = NewEntry::directory(name, Metadata::default());
    root_entry.physical_size = Some(0);
    let mut tree = FileTree::with_root(root_entry);
    let mut pending: Vec<(PathBuf, NodeId)> = vec![(root.to_path_buf(), NodeId::ROOT)];
    while let Some((dir, parent)) = pending.pop() {
        for entry in fs::read_dir(&dir).expect("read_dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            let meta = fs::symlink_metadata(&path).expect("metadata");
            if meta.is_dir() {
                let mut folder = NewEntry::directory(entry.file_name(), Metadata::default());
                folder.physical_size = Some(0);
                let id = tree.add_child(parent, folder).expect("add dir");
                pending.push((path, id));
            } else {
                let kind = if meta.file_type().is_symlink() {
                    EntryKind::Symlink
                } else {
                    EntryKind::File
                };
                let size = if kind == EntryKind::File {
                    meta.len()
                } else {
                    0
                };
                let file = NewEntry {
                    name: entry.file_name(),
                    kind,
                    logical_size: size,
                    physical_size: Some(size),
                    identity: None,
                    hardlink_duplicate: false,
                    unreadable: false,
                    metadata: Metadata::default(),
                };
                tree.add_child(parent, file).expect("add file");
            }
        }
    }
    tree.roll_up();
    tree
}

/// Directory on disk that holds the tree root.
pub fn base(root: &Path) -> PathBuf {
    tree_base(root)
}

/// The attached node whose tree path is `<root name>/<relative>`.
pub fn find(tree: &FileTree, root: &Path, relative: &str) -> Option<NodeId> {
    let wanted = PathBuf::from(root.file_name().expect("root name")).join(relative);
    tree.attached_ids()
        .find(|id| tree.path_of(*id).as_deref() == Some(wanted.as_path()))
}

pub fn find_required(tree: &FileTree, root: &Path, relative: &str) -> NodeId {
    find(tree, root, relative).unwrap_or_else(|| panic!("no attached node {relative}"))
}
