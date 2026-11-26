//! Temp-dir fixtures paired with synthetic scanner trees.
#![allow(dead_code, reason = "each integration test uses a helper subset")]

use std::{
    fs,
    path::{Path, PathBuf},
};

use tempfile::TempDir;
use timberstat_dev_storage::{
    DevLocationSet, DevStorageReport, LocationRole, analyse_with_locations,
};
use timberstat_filesystem::{EntryKind, FileTree, Metadata, NewEntry, NodeId};

pub struct Fixture {
    temp: TempDir,
    tree: FileTree,
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
        Self {
            temp,
            tree: FileTree::with_root(root),
        }
    }

    pub fn root_path(&self) -> &Path {
        self.temp.path()
    }

    pub fn path(&self, id: NodeId) -> PathBuf {
        if id == NodeId::ROOT {
            return self.root_path().to_path_buf();
        }
        self.tree.path_of(id).map_or_else(PathBuf::new, |relative| {
            let root_name = self.tree.node(NodeId::ROOT).expect("root").name();
            relative
                .strip_prefix(root_name)
                .map_or(relative.clone(), |suffix| self.root_path().join(suffix))
        })
    }

    pub fn dir(&mut self, parent: NodeId, name: &str) -> NodeId {
        let path = self.path(parent).join(name);
        fs::create_dir(&path).expect("create fixture directory");
        let mut entry = NewEntry::directory(name.into(), Metadata::default());
        entry.physical_size = Some(0);
        self.tree.add_child(parent, entry).expect("add directory")
    }

    pub fn dirs(&mut self, parent: NodeId, names: &[&str]) -> NodeId {
        names
            .iter()
            .fold(parent, |parent, name| self.dir(parent, name))
    }

    pub fn file(&mut self, parent: NodeId, name: &str, logical_size: u64) -> NodeId {
        self.file_with_physical(parent, name, logical_size, Some(logical_size))
    }

    pub fn file_with_physical(
        &mut self,
        parent: NodeId,
        name: &str,
        logical_size: u64,
        physical_size: Option<u64>,
    ) -> NodeId {
        let path = self.path(parent).join(name);
        fs::write(&path, b"fixture").expect("write fixture file");
        let entry = NewEntry {
            name: name.into(),
            kind: EntryKind::File,
            logical_size,
            physical_size,
            identity: None,
            hardlink_duplicate: false,
            unreadable: false,
            metadata: Metadata::default(),
        };
        self.tree.add_child(parent, entry).expect("add file")
    }

    pub fn mark_unreadable(&mut self, id: NodeId) {
        self.tree.mark_unreadable(id).expect("mark unreadable");
    }

    pub fn finish(mut self) -> (TempDir, FileTree) {
        self.tree.roll_up();
        (self.temp, self.tree)
    }
}

pub fn locations(root: &Path) -> DevLocationSet {
    DevLocationSet::new()
        .add_directory(
            LocationRole::NpmCache,
            root.join("npm-cache"),
            "test npm cache",
        )
        .add_directory(
            LocationRole::PnpmCache,
            root.join("pnpm-store"),
            "test pnpm store",
        )
        .add_directory(
            LocationRole::YarnCache,
            root.join("yarn-cache"),
            "test Yarn cache",
        )
        .add_directory(
            LocationRole::PipCache,
            root.join("pip-cache"),
            "test pip cache",
        )
        .add_directory(
            LocationRole::GradleCache,
            root.join("gradle/caches"),
            "test Gradle caches",
        )
        .add_directory(
            LocationRole::MavenRepository,
            root.join("maven/.m2"),
            "test Maven .m2",
        )
        .add_file(
            LocationRole::DockerStorage,
            root.join("Docker/wsl/data/ext4.vhdx"),
            "test Docker Desktop VHDX",
        )
}

pub fn complete_fixture() -> (TempDir, FileTree, DevLocationSet) {
    let mut fixture = Fixture::new();
    let root = NodeId::ROOT;

    let repo = fixture.dir(root, "repo");
    fixture.dir(repo, ".git");
    fixture.file(repo, "Cargo.toml", 5);
    let src = fixture.dir(repo, "src");
    fixture.file(src, "lib.rs", 40);
    let target = fixture.dir(repo, "target");
    fixture.file(target, "CACHEDIR.TAG", 3);
    let cargo_debug = fixture.dir(target, "debug");
    fixture.file(cargo_debug, "libexample.rlib", 300);

    let rust_debug = fixture.dirs(root, &["rust-output", "debug"]);
    fixture.file(rust_debug, "orphan.rlib", 80);
    let node_modules = fixture.dirs(root, &["node-app", "node_modules", "package"]);
    fixture.file(node_modules, "index.js", 90);

    for (path, name, size) in [
        (&["npm-cache"][..], "npm.bin", 20),
        (&["pnpm-store"][..], "pnpm.bin", 21),
        (&["yarn-cache"][..], "yarn.bin", 22),
        (&["pip-cache"][..], "pip.bin", 23),
        (&["gradle", "caches"][..], "gradle.bin", 24),
    ] {
        let directory = fixture.dirs(root, path);
        fixture.file(directory, name, size);
    }
    let maven = fixture.dirs(root, &["maven", ".m2"]);
    fixture.file(maven, "settings.xml", 7);
    let repository = fixture.dir(maven, "repository");
    fixture.file(repository, "artifact.jar", 25);

    let venv = fixture.dir(root, "venv");
    fixture.file(venv, "pyvenv.cfg", 4);
    let lib = fixture.dir(venv, "lib");
    fixture.file(lib, "python.py", 50);
    let cmake = fixture.dir(root, "cmake-build");
    fixture.file(cmake, "CMakeCache.txt", 5);
    fixture.file(cmake, "liboutput.a", 60);

    let vs = fixture.dir(root, "vs-app");
    fixture.file(vs, "App.sln", 5);
    let vs_debug = fixture.dirs(vs, &["x64", "Debug"]);
    fixture.file(vs_debug, "app.obj", 70);
    let docker = fixture.dirs(root, &["Docker", "wsl", "data"]);
    fixture.file(docker, "ext4.vhdx", 200);
    let vm = fixture.dir(root, "vm");
    fixture.file(vm, "disk.qcow2", 220);

    let locations = locations(fixture.root_path());
    let (temp, tree) = fixture.finish();
    (temp, tree, locations)
}

pub fn complete_report() -> (TempDir, DevStorageReport) {
    let (temp, tree, locations) = complete_fixture();
    let report = analyse_with_locations(&tree, temp.path(), &locations);
    (temp, report)
}

pub fn analyse(fixture: Fixture) -> (TempDir, DevStorageReport) {
    let locations = locations(fixture.root_path());
    let (temp, tree) = fixture.finish();
    let report = analyse_with_locations(&tree, temp.path(), &locations);
    (temp, report)
}
