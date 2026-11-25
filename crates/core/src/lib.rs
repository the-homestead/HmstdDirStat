//! GPUI-independent domain primitives for HmstdDirStat.
//!
//! Nothing in this crate may depend on GPUI or on any platform UI API
//! (ARCHITECTURE §1: scanner and visualization independence).

pub mod config;
pub mod error;
pub mod logging;

pub use config::{AppConfig, ThemePreference};
pub use error::ConfigError;
