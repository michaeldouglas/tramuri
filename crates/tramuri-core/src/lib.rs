//! Shared domain API for Tramuri's local-first project discovery.

mod discovery;
mod error;
mod storage;

pub use discovery::{FileRecord, HashStatus, InitReport, ScanReport, ScanWarning, init, scan};
pub use error::{Result, TramuriError};

/// The current on-disk SQLite schema version.
pub const SCHEMA_VERSION: u32 = 1;

