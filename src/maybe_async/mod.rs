//! This module defines traits with async methods, but the underlying implementation
//! may be async (i.e. reqwest and tokio) or blocking (ureq and std).  The top
//! level API will instantiate a normal async implementation of these traits
//! (found in `crate::nonblocking::imp`), or a blocking implementation (found in
//! `crate::blocking::imp`).  In the blocking case, the top-level future will be
//! handed off to `sync_executor` so all 'futures" in the blocking implementation
//! must resolve immediately.
//!

mod file;
mod http_client;
mod limiter;
mod system;

pub use file::*;
pub use http_client::*;
pub use limiter::*;
pub use system::*;
