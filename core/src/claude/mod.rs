//! Everything specific to Claude Desktop. See docs/01_project_brief.md for how
//! third-party mode works and where these facts come from.

pub mod icon;
pub mod library;
pub mod models;
pub mod paths;
pub mod process;

pub use library::{DeploymentMode, GatewayProfile, Snapshot};
pub use paths::ClaudePaths;
pub use process::ClaudeApp;
