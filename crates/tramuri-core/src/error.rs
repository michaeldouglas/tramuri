use std::path::PathBuf;

/// Errors returned by Tramuri's public Rust API.
#[derive(Debug, thiserror::Error)]
pub enum TramuriError {
    #[error("project directory does not exist or is not a directory: {0}")]
    InvalidProjectRoot(PathBuf),
    #[error("project is not initialized at {0}; run `tramuri init` first")]
    NotInitialized(PathBuf),
    #[error("Tramuri state path must be a real directory or regular file, not a symlink: {0}")]
    InvalidStatePath(PathBuf),
    #[error("unsupported Tramuri database schema version {found}; this build supports {supported}")]
    UnsupportedSchema { found: i64, supported: u32 },
    #[error("path cannot be represented as UTF-8: {0:?}")]
    NonUtf8Path(PathBuf),
    #[error("failed to {operation} `{path}`: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

pub type Result<T> = std::result::Result<T, TramuriError>;
