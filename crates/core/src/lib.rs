//! GPUI-independent domain primitives for TimberStat.
//!
//! Nothing in this crate may depend on GPUI or on any platform UI API
//! (ARCHITECTURE §1: scanner and visualization independence).

pub mod command;
pub mod config;
pub mod error;
pub mod layout_resize;
pub mod logging;
pub mod scan_mode;
pub mod timing;
pub mod workspace;

pub use config::{AppConfig, ThemePreference};
pub use error::ConfigError;
