use std::path::{Path, PathBuf};

use rusqlite::{Connection, TransactionBehavior, params};

use crate::{HashStatus, Result, SCHEMA_VERSION, TramuriError};

pub(crate) const APP_DIR: &str = ".tramuri";
pub(crate) const DATABASE_FILE: &str = "tramuri.sqlite3";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredFile {
    pub path: String,
    pub size_bytes: u64,
    pub modified_unix_ms: Option<i64>,
    pub sha256: Option<String>,
    pub hash_status: HashStatus,
}

pub(crate) fn database_path(root: &Path) -> PathBuf {
    root.join(APP_DIR).join(DATABASE_FILE)
}

pub(crate) fn open_database(root: &Path) -> Result<Connection> {
    let app_dir = root.join(APP_DIR);
    let app_metadata = match std::fs::symlink_metadata(&app_dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(TramuriError::NotInitialized(database_path(root)));
        }
        Err(source) => {
            return Err(TramuriError::Io {
                operation: "inspect Tramuri state directory",
                path: app_dir,
                source,
            });
        }
    };
    if app_metadata.file_type().is_symlink() || !app_metadata.is_dir() {
        return Err(TramuriError::InvalidStatePath(app_dir));
    }

    let path = database_path(root);
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(TramuriError::NotInitialized(path));
        }
        Err(source) => {
            return Err(TramuriError::Io {
                operation: "inspect Tramuri database",
                path,
                source,
            });
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(TramuriError::InvalidStatePath(path));
    }
    let connection = Connection::open(&path)?;
    validate_schema(&connection)?;
    Ok(connection)
}

pub(crate) fn initialize_database(root: &Path) -> Result<bool> {
    let app_dir = root.join(APP_DIR);
    match std::fs::symlink_metadata(&app_dir) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(TramuriError::InvalidStatePath(app_dir));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir(&app_dir).map_err(|source| TramuriError::Io {
                operation: "create Tramuri state directory",
                path: app_dir.clone(),
                source,
            })?;
        }
        Err(source) => {
            return Err(TramuriError::Io {
                operation: "inspect Tramuri state directory",
                path: app_dir,
                source,
            });
        }
    }

    let path = database_path(root);
    let already_initialized = match std::fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err(TramuriError::InvalidStatePath(path));
        }
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(source) => {
            return Err(TramuriError::Io {
                operation: "inspect Tramuri database",
                path,
                source,
            });
        }
    };
    let mut connection = Connection::open(&path)?;
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    match version {
        0 => {
            if already_initialized {
                let user_tables: i64 = connection.query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
                    [],
                    |row| row.get(0),
                )?;
                if user_tables != 0 {
                    return Err(TramuriError::UnsupportedSchema {
                        found: 0,
                        supported: SCHEMA_VERSION,
                    });
                }
            }
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute_batch(
                "CREATE TABLE IF NOT EXISTS file_inventory (
                    path TEXT PRIMARY KEY NOT NULL,
                    size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
                    modified_unix_ms INTEGER,
                    sha256 TEXT,
                    hash_status TEXT NOT NULL CHECK (hash_status IN ('hashed', 'too_large', 'read_error'))
                ) WITHOUT ROWID;
                PRAGMA user_version = 1;",
            )?;
            transaction.commit()?;
        }
        version if version == i64::from(SCHEMA_VERSION) => {}
        found => {
            return Err(TramuriError::UnsupportedSchema {
                found,
                supported: SCHEMA_VERSION,
            });
        }
    }

    Ok(already_initialized)
}

fn validate_schema(connection: &Connection) -> Result<()> {
    let found: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if found != i64::from(SCHEMA_VERSION) {
        return Err(TramuriError::UnsupportedSchema {
            found,
            supported: SCHEMA_VERSION,
        });
    }
    Ok(())
}

pub(crate) fn load_inventory(connection: &Connection) -> Result<Vec<StoredFile>> {
    let mut statement = connection.prepare(
        "SELECT path, size_bytes, modified_unix_ms, sha256, hash_status
         FROM file_inventory ORDER BY path",
    )?;
    let rows = statement.query_map([], |row| {
        let size_bytes: i64 = row.get(1)?;
        let hash_status_value: String = row.get(4)?;
        let hash_status = HashStatus::from_storage_value(&hash_status_value).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("unknown stored hash status: {hash_status_value}"),
                )
                .into(),
            )
        })?;
        Ok(StoredFile {
            path: row.get(0)?,
            size_bytes: u64::try_from(size_bytes)
                .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(1, size_bytes))?,
            modified_unix_ms: row.get(2)?,
            sha256: row.get(3)?,
            hash_status,
        })
    })?;

    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(TramuriError::from)
}

pub(crate) fn persist_scan(
    connection: &mut Connection,
    files: &[StoredFile],
    deleted_paths: &[String],
    complete: bool,
) -> Result<()> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if complete {
        transaction.execute("DELETE FROM file_inventory", [])?;
    }

    for file in files
        .iter()
        .filter(|file| file.hash_status != HashStatus::ReadError)
    {
        let size_bytes = i64::try_from(file.size_bytes)
            .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(1, i64::MAX))?;
        transaction.execute(
            "INSERT INTO file_inventory (path, size_bytes, modified_unix_ms, sha256, hash_status)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(path) DO UPDATE SET
                 size_bytes = excluded.size_bytes,
                 modified_unix_ms = excluded.modified_unix_ms,
                 sha256 = excluded.sha256,
                 hash_status = excluded.hash_status",
            params![
                file.path,
                size_bytes,
                file.modified_unix_ms,
                file.sha256,
                file.hash_status.as_storage_value(),
            ],
        )?;
    }

    if complete {
        for path in deleted_paths {
            transaction.execute("DELETE FROM file_inventory WHERE path = ?1", [path])?;
        }
    }

    transaction.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::{StoredFile, load_inventory, persist_scan};
    use crate::{HashStatus, TramuriError};

    fn inventory_database() -> Connection {
        let connection = Connection::open_in_memory().expect("in-memory database should open");
        connection
            .execute_batch(
                "CREATE TABLE file_inventory (
                    path TEXT PRIMARY KEY NOT NULL,
                    size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
                    modified_unix_ms INTEGER,
                    sha256 TEXT,
                    hash_status TEXT NOT NULL CHECK (hash_status IN ('hashed', 'too_large', 'read_error'))
                ) WITHOUT ROWID;",
            )
            .expect("inventory table should be created");
        connection
    }

    #[test]
    fn persisted_hash_statuses_round_trip_without_storing_read_errors() {
        let mut connection = inventory_database();
        let files = [
            StoredFile {
                path: "hashed.txt".to_owned(),
                size_bytes: 4,
                modified_unix_ms: Some(1),
                sha256: Some("abcd".to_owned()),
                hash_status: HashStatus::Hashed,
            },
            StoredFile {
                path: "large.bin".to_owned(),
                size_bytes: 32 * 1024 * 1024 + 1,
                modified_unix_ms: Some(2),
                sha256: None,
                hash_status: HashStatus::TooLarge,
            },
            StoredFile {
                path: "unreadable.txt".to_owned(),
                size_bytes: 0,
                modified_unix_ms: None,
                sha256: None,
                hash_status: HashStatus::ReadError,
            },
        ];

        persist_scan(&mut connection, &files, &[], true).expect("scan should persist");
        let loaded = load_inventory(&connection).expect("inventory should load");

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].hash_status, HashStatus::Hashed);
        assert_eq!(loaded[1].hash_status, HashStatus::TooLarge);
        assert_eq!(loaded[0].path, "hashed.txt");
        assert_eq!(loaded[1].path, "large.bin");
    }

    #[test]
    fn unknown_database_hash_status_returns_a_conversion_error() {
        let connection = inventory_database();
        connection
            .pragma_update(None, "ignore_check_constraints", true)
            .expect("test should be able to bypass the schema check");
        connection
            .execute(
                "INSERT INTO file_inventory (path, size_bytes, modified_unix_ms, sha256, hash_status)
                 VALUES ('unknown.txt', 1, NULL, NULL, 'future_status')",
                [],
            )
            .expect("invalid value should be inserted for this diagnostic test");

        let error = load_inventory(&connection).expect_err("unknown status must not be accepted");

        assert!(matches!(
            error,
            TramuriError::Sqlite(rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                _
            ))
        ));
    }
}
