pub mod config;
pub mod filter;
pub mod runner;
pub mod verify;

/// Core version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
