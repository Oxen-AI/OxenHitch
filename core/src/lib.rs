//! Hitch core: everything that is not UI.
//!
//! `claude` knows how Claude Desktop stores its third-party config.
//! `gateway` talks to the Oxen gateway. `hitch` ties them together.

pub mod claude;
pub mod error;
pub mod gateway;
pub mod hitch;
pub mod secrets;
pub mod state;

pub use error::{Error, Result};
