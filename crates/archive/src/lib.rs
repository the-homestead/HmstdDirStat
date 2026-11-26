//! Public API for the `archive` crate.
//
//! This crate provides a thin wrapper around the `hmstd-fileops` `Control` trait
//! and concrete handler implementations for various archive formats. The
//! top‑level `archive` module re‑exports the public types and functions.
#![allow(dead_code)]

pub mod engine;
pub mod error;
pub mod format;
pub mod handler;
pub mod ops;
pub mod parse;
pub mod runner;

pub use engine::ArchiveListing;
pub use engine::ArchiveManager;
pub use error::ArchiveError;
pub use format::ArchiveFormat;
pub use handler::HandlerCatalog;
pub use ops::CommandSpec;
pub use runner::ProcessOutcome;
