use std::ffi::OsStr;

/// Stable index of a node inside a [`FileTree`](crate::FileTree).
///
/// Children are always appended after their parent, so a node's id is greater
/// than its parent's. Aggregation relies on this ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(u32);

impl NodeId {
    /// The root directory of every tree.
    pub const ROOT: NodeId = NodeId(0);

    pub fn index(self) -> usize {
        self.0 as usize
    }

    pub fn from_index(index: usize) -> Option<NodeId> {
        u32::try_from(index).ok().map(NodeId)
    }
}

/// What a directory entry is. Symlinks and junctions are never followed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Directory,
    Symlink,
    /// Devices, FIFOs, sockets and other special entries. Never opened.
    Other,
}

/// Identity of a file's storage object, used to count hard links once.
///
/// On Unix this is `(st_dev, st_ino)`. On Windows it will be
/// `(volume serial, file id)` once the Windows identity source lands (Phase 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileIdentity {
    pub volume: u64,
    pub file_id: u64,
}

/// Metadata captured at scan time. Fields a platform does not provide are `None`/`false`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Metadata {
    /// Last modification time as seconds since the Unix epoch (negative before 1970).
    pub modified_unix_secs: Option<i64>,
    pub readonly: bool,
    /// POSIX permission bits (`st_mode`), Unix only.
    pub unix_mode: Option<u32>,
    /// Raw `FILE_ATTRIBUTE_*` bits, Windows only.
    pub windows_attributes: Option<u32>,
}

const WINDOWS_ATTRIBUTE_HIDDEN: u32 = 0x0000_0002;
const WINDOWS_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

impl Metadata {
    /// True when the Windows reparse-point attribute is set (symlink, junction, cloud placeholder, ...).
    pub fn is_reparse_point(&self) -> bool {
        self.windows_attributes
            .is_some_and(|bits| bits & WINDOWS_ATTRIBUTE_REPARSE_POINT != 0)
    }

    /// True when the Windows hidden attribute is set.
    pub fn is_hidden_attribute(&self) -> bool {
        self.windows_attributes
            .is_some_and(|bits| bits & WINDOWS_ATTRIBUTE_HIDDEN != 0)
    }
}

/// Aggregated figures for a node and everything beneath it.
///
/// Logical and physical accounting are kept separate. `physical_known` is false
/// when any entry in the subtree has no physical size on this platform, and
/// `complete` is false when any directory in the subtree could not be read.
/// Views must show the totals as partial in either case.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Totals {
    /// Sum of apparent file sizes. Each path counts its own size, so hard links count once per path.
    pub logical: u64,
    /// Sum of physical charges. Each distinct file identity is charged once.
    pub physical: u64,
    pub physical_known: bool,
    pub files: u64,
    pub directories: u64,
    pub complete: bool,
}

/// One entry in the tree. Fields are read through accessors; only the tree mutates them.
#[derive(Debug, Clone)]
pub struct Node {
    pub(crate) name: Box<OsStr>,
    pub(crate) parent: Option<NodeId>,
    pub(crate) first_child: Option<NodeId>,
    pub(crate) last_child: Option<NodeId>,
    pub(crate) next_sibling: Option<NodeId>,
    pub(crate) kind: EntryKind,
    pub(crate) logical_size: u64,
    pub(crate) physical_size: Option<u64>,
    pub(crate) identity: Option<FileIdentity>,
    pub(crate) hardlink_duplicate: bool,
    pub(crate) unreadable: bool,
    pub(crate) metadata: Metadata,
    pub(crate) totals: Totals,
    /// Set on every node of a subtree removed by [`FileTree::detach`].
    pub(crate) detached: bool,
}

impl Node {
    pub fn name(&self) -> &OsStr {
        &self.name
    }

    pub fn parent(&self) -> Option<NodeId> {
        self.parent
    }

    pub fn kind(&self) -> EntryKind {
        self.kind
    }

    /// This entry's own apparent size. Zero for directories.
    pub fn logical_size(&self) -> u64 {
        self.logical_size
    }

    /// This entry's own physical charge, or `None` if the platform cannot report it.
    /// A later hard link to an already-charged file reports `Some(0)`.
    pub fn physical_size(&self) -> Option<u64> {
        self.physical_size
    }

    pub fn identity(&self) -> Option<FileIdentity> {
        self.identity
    }

    /// True when this path is a further hard link to a file already charged elsewhere.
    pub fn is_hardlink_duplicate(&self) -> bool {
        self.hardlink_duplicate
    }

    /// True when this directory could not be listed, or one of its entries could not be read.
    pub fn is_unreadable(&self) -> bool {
        self.unreadable
    }

    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    /// Subtree aggregates. Valid only after [`FileTree::roll_up`](crate::FileTree::roll_up).
    pub fn totals(&self) -> Totals {
        self.totals
    }
}
