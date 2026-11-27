//! Reproducible synthetic datasets for the scan benchmarks.
//!
//! Every dataset is written under the system temp directory only, in a folder
//! whose name starts with [`ROOT_PREFIX`]. Roots anywhere else are refused, so
//! no benchmark can write into a user's drives or project folders. Contents are
//! deterministic: the same [`Kind`] and [`GENERATOR_VERSION`] always produce the
//! same tree, sizes and manifest.

use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

/// Bump when the shape of any dataset changes, so old manifests are not reused.
pub const GENERATOR_VERSION: u32 = 1;
/// Fixed seed for the file-size sequence.
pub const SEED: u64 = 0x5449_4D42_4552_5354;
/// Every generated dataset root starts with this, and roots without it are refused.
pub const ROOT_PREFIX: &str = "timberstat-bench-";
/// Directory nesting of the deep dataset (the spec asks for more than 200).
pub const DEEP_LEVELS: u32 = 220;
/// Logical size of each few-huge file.
pub const HUGE_FILE_BYTES: u64 = 1 << 30;
/// Number of few-huge files.
pub const HUGE_FILE_COUNT: u32 = 4;

const MAX_SMALL_BYTES: u64 = 4096;
const CHUNK: usize = 4096;
const MIB: u64 = 1 << 20;

/// The dataset shapes the benchmark suite knows about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// 300 files in one flat folder.
    Small,
    /// 100 000 files of 16 bytes in 100 folders. Same shape as the ignored scanner scale test.
    HundredK,
    /// 1 000 000 files of 0-4 KiB in 1 000 folders.
    OneMillion,
    /// A single chain of 220 nested folders, two files per level.
    Deep,
    /// 20 000 files of 0-4 KiB in 20 folders.
    ManySmall,
    /// Four 1 GiB logical files in one folder.
    FewHuge,
}

impl Kind {
    pub const ALL: [Kind; 6] = [
        Kind::Small,
        Kind::HundredK,
        Kind::OneMillion,
        Kind::Deep,
        Kind::ManySmall,
        Kind::FewHuge,
    ];

    /// Stable id used in folder names, manifests and result files.
    pub fn id(self) -> &'static str {
        match self {
            Kind::Small => "small",
            Kind::HundredK => "100k",
            Kind::OneMillion => "1m",
            Kind::Deep => "deep",
            Kind::ManySmall => "many-small",
            Kind::FewHuge => "few-huge",
        }
    }

    pub fn parse(id: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|kind| kind.id() == id)
    }

    pub fn description(self) -> &'static str {
        match self {
            Kind::Small => "300 files, flat, 0-4 KiB",
            Kind::HundredK => "100000 files, 100 folders, 16 bytes each",
            Kind::OneMillion => "1000000 files, 1000 folders, 0-4 KiB",
            Kind::Deep => "220 nested folders, 2 files of 32 bytes per level",
            Kind::ManySmall => "20000 files, 20 folders, 0-4 KiB",
            Kind::FewHuge => "4 sparse-length files of 1 GiB each (logical size)",
        }
    }

    /// Default location of this dataset: `<temp>/timberstat-bench-<id>`.
    pub fn default_root(self) -> PathBuf {
        std::env::temp_dir().join(format!("{ROOT_PREFIX}{}", self.id()))
    }
}

/// Files counted in one size class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SizeClass {
    pub label: String,
    pub files: u64,
}

/// What was generated. Written next to the dataset as `<root>.manifest.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub dataset: String,
    pub description: String,
    pub generator_version: u32,
    pub seed: u64,
    pub root: String,
    pub files: u64,
    /// Sub-folders created, not counting the root.
    pub directories: u64,
    /// Nodes a complete scan should report: files + directories + the root.
    pub expected_tree_nodes: u64,
    pub logical_bytes: u64,
    /// Deepest folder level holding an entry (root = 0).
    pub max_depth: u32,
    pub size_classes: Vec<SizeClass>,
}

#[derive(Debug, thiserror::Error)]
pub enum DatasetError {
    #[error("refusing {0}: dataset roots must sit directly inside the temp directory {1}")]
    OutsideTemp(PathBuf, PathBuf),
    #[error("refusing {0}: dataset folder names must start with \"{ROOT_PREFIX}\"")]
    BadName(PathBuf),
    #[error("{0} already exists and is not empty; remove it first or pass --clean")]
    NotEmpty(PathBuf),
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

fn io_at(path: &Path) -> impl FnOnce(io::Error) -> DatasetError + '_ {
    move |source| DatasetError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Resolves `root` to an absolute path and checks it is a permitted dataset root:
/// the parent must be the temp directory or inside it, and the name must start
/// with [`ROOT_PREFIX`]. Returns the resolved path.
pub fn check_root(root: &Path) -> Result<PathBuf, DatasetError> {
    let temp = std::env::temp_dir();
    let temp_real = fs::canonicalize(&temp).map_err(io_at(&temp))?;
    let name = root
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| n.starts_with(ROOT_PREFIX))
        .ok_or_else(|| DatasetError::BadName(root.to_path_buf()))?;
    let parent = root
        .parent()
        .ok_or_else(|| DatasetError::BadName(root.to_path_buf()))?;
    let parent_real = fs::canonicalize(parent).map_err(io_at(parent))?;
    if !parent_real.starts_with(&temp_real) {
        return Err(DatasetError::OutsideTemp(root.to_path_buf(), temp_real));
    }
    Ok(parent_real.join(name))
}

/// Path of the manifest written beside a dataset root.
pub fn manifest_path(root: &Path) -> PathBuf {
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    root.with_file_name(format!("{name}.manifest.json"))
}

/// Reads the manifest of an existing dataset, if one was written.
pub fn read_manifest(root: &Path) -> Option<Manifest> {
    let text = fs::read_to_string(manifest_path(root)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Generates `kind` at `root` and writes its manifest. Refuses a non-empty existing root.
pub fn generate(kind: Kind, root: &Path) -> Result<Manifest, DatasetError> {
    let root = check_root(root)?;
    if root.exists() {
        let mut entries = fs::read_dir(&root).map_err(io_at(&root))?;
        if entries.next().is_some() {
            return Err(DatasetError::NotEmpty(root));
        }
    }
    fs::create_dir_all(&root).map_err(io_at(&root))?;
    let mut builder = Builder::new(root.clone());
    match kind {
        Kind::Small => {
            let flat = builder.dir(&root, "flat")?;
            for i in 0..300 {
                let len = builder.size_below_small();
                builder.file(&flat, &format!("f{i:04}.bin"), len, 1)?;
            }
        }
        Kind::HundredK => {
            for shard in 0..100 {
                let sub = builder.dir(&root, &format!("s{shard:03}"))?;
                for i in 0..1_000 {
                    builder.file(&sub, &format!("f{i:04}.bin"), 16, 1)?;
                }
            }
        }
        Kind::OneMillion => {
            for shard in 0..1_000 {
                let sub = builder.dir(&root, &format!("s{shard:04}"))?;
                for i in 0..1_000 {
                    let len = builder.size_below_small();
                    builder.file(&sub, &format!("f{i:04}.bin"), len, 1)?;
                }
            }
        }
        Kind::ManySmall => {
            for shard in 0..20 {
                let sub = builder.dir(&root, &format!("s{shard:03}"))?;
                for i in 0..1_000 {
                    let len = builder.size_below_small();
                    builder.file(&sub, &format!("f{i:04}.bin"), len, 1)?;
                }
            }
        }
        Kind::Deep => {
            let mut current = root.clone();
            for level in 1..=DEEP_LEVELS {
                current = builder.dir(&current, "d")?;
                builder.file(&current, "a.bin", 32, level)?;
                builder.file(&current, "b.bin", 32, level)?;
            }
        }
        Kind::FewHuge => {
            let huge = builder.dir(&root, "huge")?;
            for i in 0..HUGE_FILE_COUNT {
                builder.sparse_length_file(&huge, &format!("big{i}.bin"), HUGE_FILE_BYTES, 1)?;
            }
        }
    }
    let manifest = builder.finish(kind);
    let json = serde_json::to_string_pretty(&manifest).expect("manifest serialises");
    let path = manifest_path(&root);
    fs::write(&path, json).map_err(io_at(&path))?;
    Ok(manifest)
}

/// Removes a dataset root and its manifest after the same path checks as [`generate`].
pub fn clean(root: &Path) -> Result<(), DatasetError> {
    let root = check_root(root)?;
    if root.exists() {
        fs::remove_dir_all(&root).map_err(io_at(&root))?;
    }
    let manifest = manifest_path(&root);
    if manifest.exists() {
        fs::remove_file(&manifest).map_err(io_at(&manifest))?;
    }
    Ok(())
}

/// Returns the default dataset for `kind`, generating it first if no manifest exists.
pub fn ensure(kind: Kind) -> Result<(PathBuf, Manifest), DatasetError> {
    let root = kind.default_root();
    if root.is_dir()
        && let Some(manifest) = read_manifest(&root)
        && manifest.generator_version == GENERATOR_VERSION
        && manifest.dataset == kind.id()
    {
        return Ok((root, manifest));
    }
    let manifest = generate(kind, &root)?;
    Ok((root, manifest))
}

/// SplitMix64: small, deterministic, no dependency.
struct SplitMix64(u64);

impl SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

struct Builder {
    root: PathBuf,
    rng: SplitMix64,
    files: u64,
    directories: u64,
    logical_bytes: u64,
    max_depth: u32,
    classes: [u64; 4],
}

impl Builder {
    fn new(root: PathBuf) -> Self {
        Self {
            root,
            rng: SplitMix64(SEED),
            files: 0,
            directories: 0,
            logical_bytes: 0,
            max_depth: 0,
            classes: [0; 4],
        }
    }

    /// Uniform length in `0..=4096`.
    fn size_below_small(&mut self) -> u64 {
        self.rng.next_u64() % (MAX_SMALL_BYTES + 1)
    }

    fn dir(&mut self, parent: &Path, name: &str) -> Result<PathBuf, DatasetError> {
        let path = parent.join(name);
        fs::create_dir(&path).map_err(io_at(&path))?;
        self.directories += 1;
        Ok(path)
    }

    fn file(&mut self, dir: &Path, name: &str, len: u64, level: u32) -> Result<(), DatasetError> {
        let path = dir.join(name);
        let mut out = File::create(&path).map_err(io_at(&path))?;
        let chunk = [b'x'; CHUNK];
        let mut left = len;
        while left > 0 {
            let n = left.min(CHUNK as u64) as usize;
            out.write_all(&chunk[..n]).map_err(io_at(&path))?;
            left -= n as u64;
        }
        self.account(len, level);
        Ok(())
    }

    /// A file whose logical length is set without writing its contents.
    fn sparse_length_file(
        &mut self,
        dir: &Path,
        name: &str,
        len: u64,
        level: u32,
    ) -> Result<(), DatasetError> {
        let path = dir.join(name);
        let out = File::create(&path).map_err(io_at(&path))?;
        out.set_len(len).map_err(io_at(&path))?;
        self.account(len, level);
        Ok(())
    }

    fn account(&mut self, len: u64, level: u32) {
        self.files += 1;
        self.logical_bytes += len;
        self.max_depth = self.max_depth.max(level);
        let class = match len {
            0 => 0,
            1..=MAX_SMALL_BYTES => 1,
            _ if len <= MIB => 2,
            _ => 3,
        };
        self.classes[class] += 1;
    }

    fn finish(self, kind: Kind) -> Manifest {
        let labels = ["0 B", "1 B-4 KiB", "4 KiB-1 MiB", "over 1 MiB"];
        Manifest {
            dataset: kind.id().to_owned(),
            description: kind.description().to_owned(),
            generator_version: GENERATOR_VERSION,
            seed: SEED,
            root: self
                .root
                .display()
                .to_string()
                .trim_start_matches(r"\\?\")
                .to_owned(),
            files: self.files,
            directories: self.directories,
            expected_tree_nodes: self.files + self.directories + 1,
            logical_bytes: self.logical_bytes,
            max_depth: self.max_depth,
            size_classes: labels
                .iter()
                .zip(self.classes)
                .map(|(label, files)| SizeClass {
                    label: (*label).to_owned(),
                    files,
                })
                .collect(),
        }
    }
}
