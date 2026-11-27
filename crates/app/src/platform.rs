//! Platform facts the shell shows to the user (Plan §17).
//!
//! Windows wording is unchanged. Linux names the freedesktop or XDG equivalent, or says that a
//! feature is unavailable and why. Nothing here changes what the code does; the scan and file
//! operation paths decide that on their own.

/// Whether Fast NTFS (reading the NTFS master file table) exists on this platform. It needs an
/// NTFS volume, Windows administrator approval and the Windows helper, so it is Windows only.
pub use timberstat_engine::FAST_NTFS_AVAILABLE;

/// Tooltip for the Fast NTFS control when it is not available.
pub const FAST_NTFS_UNAVAILABLE: &str = "Fast NTFS reads the Windows NTFS file table. It is not available on this system. Walk works everywhere.";

/// Name of the system trash used by "delete to trash".
#[cfg(windows)]
pub const TRASH_NAME: &str = "Recycle Bin";
/// Name of the system trash used by "delete to trash" (freedesktop trash spec).
#[cfg(not(windows))]
pub const TRASH_NAME: &str = "Trash";

/// Context-menu label for revealing an item. Linux opens the containing folder; it does not
/// select the item inside it.
#[cfg(windows)]
pub const REVEAL_LABEL: &str = "Show in Explorer";
/// Context-menu label for revealing an item (Linux opens the containing folder).
#[cfg(not(windows))]
pub const REVEAL_LABEL: &str = "Open containing folder";
