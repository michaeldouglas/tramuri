//! Shared domain API for Tramuri's local-first project discovery.

mod discovery;
mod error;
mod hash_status;
mod storage;

pub use discovery::{FileRecord, InitReport, ScanReport, ScanWarning, init, scan};
pub use error::{Result, TramuriError};
pub use hash_status::HashStatus;

/// The current on-disk SQLite schema version.
pub const SCHEMA_VERSION: u32 = 1;
