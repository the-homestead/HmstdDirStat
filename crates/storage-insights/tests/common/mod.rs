//! Temp-dir fixtures paired with synthetic scanner trees.
#![allow(
    dead_code,
    reason = "each integration test uses a different helper subset"
)]

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use tempfile::TempDir;
use timberstat_filesystem::{EntryKind, FileIdentity, FileTree, Metadata, NewEntry, NodeId};
use timberstat_storage_insights::{
    DAY_SECS, InsightReport, InsightsOptions, LocationRole, LocationSet, analyse_with_locations,
};

pub const NOW: i64 = 1_800_000_000;

#[derive(Debug, Clone)]
pub struct FileSpec<'a> {
    pub name: &'a str,
    pub logical: u64,
    pub allocated: Option<u64>,
    pub modified: Option<i64>,
    pub identity: Option<FileIdentity>,
    pub hardlink_duplicate: bool,
}

impl<'a> FileSpec<'a> {
    pub fn new(name: &'a str, logical: u64) -> Self {
        Self {
            name,
            logical,
            allocated: Some(logical),
            modified: Some(NOW),
            identity: None,
            hardlink_duplicate: false,
        }
    }

    pub fn allocated(mut self, allocated: Option<u64>) -> Self {
        self.allocated = allocated;
        self
    }

    pub fn age_days(mut self, days: i64) -> Self {
        self.modified = Some(NOW - days * DAY_SECS);
        self
    }

    pub fn modified(mut self, modified: Option<i64>) -> Self {
        self.modified = modified;
        self
    }

    pub fn identity(mut self, volume: u64, file_id: u64) -> Self {
        self.identity = Some(FileIdentity { volume, file_id });
        self
    }

    pub fn hardlink_duplicate(mut self, duplicate: bool) -> Self {
        self.hardlink_duplicate = duplicate;
        self
    }
}

pub struct Fixture {
    temp: TempDir,
    tree: FileTree,
    paths: HashMap<NodeId, PathBuf>,
}

impl Fixture {
    pub fn new() -> Self {
        let temp = tempfile::tempdir().expect("create temp fixture");
        let root_name = temp
            .path()
            .file_name()
            .expect("temp root name")
            .to_os_string();
        let mut root = NewEntry::directory(root_name, Metadata::default());
        root.physical_size = Some(0);
        let tree = FileTree::with_root(root);
        let mut paths = HashMap::new();
        paths.insert(NodeId::ROOT, temp.path().to_path_buf());
        Self { temp, tree, paths }
    }

    pub fn root_path(&self) -> &Path {
        self.temp.path()
    }

    pub fn path(&self, id: NodeId) -> &Path {
        &self.paths[&id]
    }

    pub fn dir(&mut self, parent: NodeId, name: &str) -> NodeId {
        let path = self.path(parent).join(name);
        fs::create_dir(&path).expect("create fixture directory");
        let mut entry = NewEntry::directory(name.into(), Metadata::default());
        entry.physical_size = Some(0);
        let id = self.tree.add_child(parent, entry).expect("add directory");
        self.paths.insert(id, path);
        id
    }

    pub fn dirs(&mut self, parent: NodeId, names: &[&str]) -> NodeId {
        names
            .iter()
            .fold(parent, |parent, name| self.dir(parent, name))
    }

    pub fn file(&mut self, parent: NodeId, spec: FileSpec<'_>) -> NodeId {
        let path = self.path(parent).join(spec.name);
        fs::write(&path, b"fixture").expect("write fixture file");
        self.insert_file(parent, spec, path)
    }

    pub fn hard_link(&mut self, parent: NodeId, spec: FileSpec<'_>, original: &Path) -> NodeId {
        let path = self.path(parent).join(spec.name);
        fs::hard_link(original, &path).expect("create fixture hard link");
        self.insert_file(parent, spec, path)
    }

    fn insert_file(&mut self, parent: NodeId, spec: FileSpec<'_>, path: PathBuf) -> NodeId {
        let entry = NewEntry {
            name: spec.name.into(),
            kind: EntryKind::File,
            logical_size: spec.logical,
            physical_size: spec.allocated,
            identity: spec.identity,
            hardlink_duplicate: spec.hardlink_duplicate,
            unreadable: false,
            metadata: Metadata {
                modified_unix_secs: spec.modified,
                ..Metadata::default()
            },
        };
        let id = self.tree.add_child(parent, entry).expect("add file");
        self.paths.insert(id, path);
        id
    }

    pub fn mark_unreadable(&mut self, id: NodeId) {
        self.tree.mark_unreadable(id).expect("mark unreadable");
    }

    pub fn finish(mut self) -> (TempDir, FileTree) {
        self.tree.roll_up();
        (self.temp, self.tree)
    }
}

pub fn options() -> InsightsOptions {
    InsightsOptions {
        temp_min_age: None,
        downloads_min_age: Some(30 * DAY_SECS),
        archives_min_age: Some(30 * DAY_SECS),
        logs_min_age: None,
        installer_min_size: 100,
        vm_image_min_size: 100,
        media_min_size: 200,
        media_min_files: 2,
        include_empty_directories: true,
        include_media_collections: true,
        include_game_installs: true,
        now_unix_secs: NOW,
    }
}

pub fn locations(root: &Path) -> LocationSet {
    LocationSet::new()
        .add(LocationRole::Temporary, root.join("Temp"), "test TEMP")
        .add(
            LocationRole::ApplicationCache,
            root.join("AppCache"),
            "test application cache",
        )
        .add(LocationRole::Logs, root.join("Logs"), "test logs")
        .add(
            LocationRole::Downloads,
            root.join("Downloads"),
            "test Downloads",
        )
        .add(
            LocationRole::CrashDumps,
            root.join("CrashDumps"),
            "test CrashDumps",
        )
        .add(
            LocationRole::GameInstalls,
            root.join("Games/Steam/steamapps/common"),
            "test Steam library",
        )
}

pub fn analyse(tree: &FileTree, root: &Path, options: &InsightsOptions) -> InsightReport {
    analyse_with_locations(tree, root, &locations(root), options)
}
