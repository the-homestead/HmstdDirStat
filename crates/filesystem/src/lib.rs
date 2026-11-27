//! Authoritative in-memory filesystem model (ARCHITECTURE §3, Plan §5).
//!
//! The scanner fills this model once; every view (tree, treemap, search, exports)
//! reads it. Nothing here depends on GPUI, on a platform API, or on the scanner.

mod node;
#[cfg(test)]
mod scale_tests;
mod tree;

pub use node::{EntryKind, FileIdentity, Metadata, Node, NodeId, Totals};
pub use tree::{Children, FileTree, NewEntry, TreeError};
