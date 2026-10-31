use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::UNIX_EPOCH;

use ignore::{DirEntry, Error as IgnoreError, WalkBuilder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::storage::{
    StoredFile, database_path, initialize_database, load_inventory, open_database, persist_scan,
};
use crate::{HashStatus, Result, SCHEMA_VERSION, TramuriError};

const MAX_HASH_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InitReport {
    pub project_root: String,
    pub database_path: String,
    pub schema_version: u32,
    pub already_initialized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileRecord {
    pub path: String,
    pub size_bytes: u64,
    pub modified_unix_ms: Option<i64>,
    pub sha256: Option<String>,
    pub hash_status: HashStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScanWarning {
    pub code: String,
    pub path: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScanReport {
    pub project_root: String,
    pub complete: bool,
    pub files: Vec<FileRecord>,
    pub added: Vec<String>,
    pub changed: Vec<String>,
    pub unchanged: Vec<String>,
    pub unverified: Vec<String>,
    pub deleted: Vec<String>,
    pub sensitive_entries_excluded: usize,
    pub warnings: Vec<ScanWarning>,
}

#[derive(Debug)]
struct WalkResult {
    files: Vec<FileRecord>,
    warnings: Vec<ScanWarning>,
    sensitive_entries_excluded: usize,
    complete: bool,
}

pub fn init(path: impl AsRef<Path>) -> Result<InitReport> {
    let root = canonical_project_root(path.as_ref())?;
    let already_initialized = initialize_database(&root)?;
    Ok(InitReport {
        project_root: path_to_string(&root)?,
        database_path: path_to_string(&database_path(&root))?,
        schema_version: SCHEMA_VERSION,
        already_initialized,
    })
}

pub fn scan(path: impl AsRef<Path>) -> Result<ScanReport> {
    let root = canonical_project_root(path.as_ref())?;
    let mut connection = open_database(&root)?;
    let previous = load_inventory(&connection)?;
    let mut previous_by_path = previous
        .into_iter()
        .map(|record| (record.path.clone(), record))
        .collect::<BTreeMap<_, _>>();

    let walked = collect_files(&root);
    let mut added = Vec::new();
    let mut changed = Vec::new();
    let mut unchanged = Vec::new();
    let mut unverified = Vec::new();
    for file in &walked.files {
        if file.hash_status == HashStatus::ReadError {
            previous_by_path.remove(&file.path);
            unverified.push(file.path.clone());
            continue;
        }
        let Some(old) = previous_by_path.remove(&file.path) else {
            added.push(file.path.clone());
            continue;
        };
        match compare_file(&old, file) {
            FileComparison::Unchanged => unchanged.push(file.path.clone()),
            FileComparison::Changed => changed.push(file.path.clone()),
            FileComparison::Unverified => unverified.push(file.path.clone()),
        }
    }

    let deleted = if walked.complete {
        previous_by_path.keys().cloned().collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    let stored = walked.files.iter().map(to_stored).collect::<Vec<_>>();
    persist_scan(&mut connection, &stored, &deleted, walked.complete)?;

    Ok(ScanReport {
        project_root: path_to_string(&root)?,
        complete: walked.complete,
        files: walked.files,
        added,
        changed,
        unchanged,
        unverified,
        deleted,
        sensitive_entries_excluded: walked.sensitive_entries_excluded,
        warnings: walked.warnings,
    })
}

fn canonical_project_root(path: &Path) -> Result<PathBuf> {
    let canonical = fs::canonicalize(path).map_err(|source| TramuriError::Io {
        operation: "resolve project directory",
        path: path.to_path_buf(),
        source,
    })?;
    if !canonical.is_dir() {
        return Err(TramuriError::InvalidProjectRoot(path.to_path_buf()));
    }
    Ok(canonical)
}

fn walk_error_path(error: &IgnoreError) -> Option<&Path> {
    match error {
        IgnoreError::Partial(errors) => errors.iter().find_map(walk_error_path),
        IgnoreError::WithPath { path, .. } => Some(path),
        IgnoreError::WithLineNumber { err, .. } | IgnoreError::WithDepth { err, .. } => {
            walk_error_path(err)
        }
        IgnoreError::Loop { child, .. } => Some(child),
        IgnoreError::Io(_)
        | IgnoreError::Glob { .. }
        | IgnoreError::UnrecognizedFileType(_)
        | IgnoreError::InvalidDefinition => None,
    }
}

fn collect_files(root: &Path) -> WalkResult {
    let mut files = Vec::new();
    let mut warnings = Vec::new();
    let sensitive_entry_count = Arc::new(AtomicUsize::new(0));
    let mut complete = true;

    let mut builder = WalkBuilder::new(root);
    builder
        .follow_links(false)
        .standard_filters(true)
        .hidden(false)
        .git_global(false)
        .require_git(false)
        .parents(true)
        .filter_entry({
            let sensitive_entry_count = Arc::clone(&sensitive_entry_count);
            move |entry| filter_entry(entry, &sensitive_entry_count)
        });

    for item in builder.build() {
        let entry = match item {
            Ok(entry) => entry,
            Err(error) => {
                complete = false;
                warnings.push(ScanWarning {
                    code: "walk_error".to_owned(),
                    path: walk_error_path(&error).and_then(|path| relative_path(root, path).ok()),
                    message: "The directory entry could not be read; this scan is partial."
                        .to_owned(),
                });
                continue;
            }
        };

        if entry.depth() == 0 {
            continue;
        }
        let Some(file_type) = entry.file_type() else {
            complete = false;
            warnings.push(warning_for(
                root,
                entry.path(),
                "metadata_error",
                "File type could not be determined.",
            ));
            continue;
        };
        if file_type.is_symlink() {
            complete = false;
            warnings.push(warning_for(
                root,
                entry.path(),
                "symlink_skipped",
                "Symlinks are not followed; this scan is partial.",
            ));
            continue;
        }
        if !file_type.is_file() {
            continue;
        }

        let relative = match relative_path(root, entry.path()) {
            Ok(relative) => relative,
            Err(_) => {
                complete = false;
                warnings.push(warning_for(
                    root,
                    entry.path(),
                    "non_utf8_path",
                    "The path is not valid UTF-8 and was omitted to avoid collapsing distinct paths.",
                ));
                continue;
            }
        };

        match inventory_file(root, entry.path(), relative.clone()) {
            Ok((record, warning)) => {
                if let Some(warning) = warning {
                    if warning.code == "read_error" {
                        complete = false;
                    }
                    warnings.push(warning);
                }
                files.push(record);
            }
            Err((code, message)) => {
                complete = false;
                warnings.push(ScanWarning {
                    code,
                    path: Some(relative.clone()),
                    message,
                });
                // A placeholder keeps the current file visible in the report.
                // Persistence deliberately retains the older row on read errors.
                files.push(FileRecord {
                    path: relative,
                    size_bytes: 0,
                    modified_unix_ms: None,
                    sha256: None,
                    hash_status: HashStatus::ReadError,
                });
            }
        }
    }

    files.sort_by(|left, right| left.path.cmp(&right.path));
    warnings.sort_by(|left, right| {
        (&left.path, &left.code, &left.message).cmp(&(&right.path, &right.code, &right.message))
    });
    WalkResult {
        files,
        warnings,
        sensitive_entries_excluded: sensitive_entry_count.load(Ordering::Relaxed),
        complete,
    }
}

fn filter_entry(entry: &DirEntry, sensitive_file_count: &AtomicUsize) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    let name = entry.file_name().to_str();
    if matches!(name, Some(".git" | ".tramuri")) {
        return false;
    }
    if name.is_some_and(is_sensitive_name) {
        sensitive_file_count.fetch_add(1, Ordering::Relaxed);
        return false;
    }
    true
}

fn is_sensitive_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name == ".env"
        || name.starts_with(".env.")
        || matches!(
            name.as_str(),
            ".netrc"
                | ".pypirc"
                | ".npmrc"
                | ".ssh"
                | ".aws"
                | ".azure"
                | ".gnupg"
                | "credentials"
                | "secrets"
                | "tokens"
                | "credentials.json"
                | "service-account.json"
                | "service_account.json"
                | "application_default_credentials.json"
        )
        || name.starts_with("credentials.")
        || name.starts_with("secrets.")
        || matches!(
            Path::new(&name)
                .extension()
                .and_then(|extension| extension.to_str()),
            Some("pem" | "key" | "p12" | "pfx" | "jks" | "kdbx")
        )
        || matches!(
            name.as_str(),
            "id_rsa" | "id_ed25519" | "id_ecdsa" | "id_dsa"
        )
}

fn inventory_file(
    root: &Path,
    path: &Path,
    relative: String,
) -> std::result::Result<(FileRecord, Option<ScanWarning>), (String, String)> {
    let canonical = fs::canonicalize(path).map_err(|_| {
        (
            "canonicalize_error".to_owned(),
            "The path could not be resolved; this scan is partial.".to_owned(),
        )
    })?;
    if !canonical.starts_with(root) {
        return Err((
            "path_outside_root".to_owned(),
            "The resolved file is outside the project root and was omitted.".to_owned(),
        ));
    }

    let metadata = fs::symlink_metadata(path).map_err(|_| {
        (
            "metadata_error".to_owned(),
            "File metadata could not be read; this scan is partial.".to_owned(),
        )
    })?;
    if metadata.file_type().is_symlink() {
        return Err((
            "symlink_skipped".to_owned(),
            "A path changed to a symlink during scanning; it was not followed.".to_owned(),
        ));
    }
    let mut file = File::open(&canonical).map_err(|_| {
        (
            "read_error".to_owned(),
            "File contents could not be opened for fingerprinting; this scan is partial."
                .to_owned(),
        )
    })?;
    let opened_metadata = file.metadata().map_err(|_| {
        (
            "metadata_error".to_owned(),
            "File metadata could not be read from the opened file; this scan is partial."
                .to_owned(),
        )
    })?;
    let canonical_after_open = fs::canonicalize(path).map_err(|_| {
        (
            "path_changed_during_scan".to_owned(),
            "The path changed while the file was opened; this scan is partial.".to_owned(),
        )
    })?;
    let path_metadata_after_open = fs::symlink_metadata(path).map_err(|_| {
        (
            "path_changed_during_scan".to_owned(),
            "The path changed while the file was opened; this scan is partial.".to_owned(),
        )
    })?;
    if canonical_after_open != canonical
        || !canonical_after_open.starts_with(root)
        || path_metadata_after_open.file_type().is_symlink()
        || !opened_metadata.is_file()
    {
        return Err((
            "path_changed_during_scan".to_owned(),
            "The file path changed during scanning or resolved outside the project; this scan is partial.".to_owned(),
        ));
    }

    let size_bytes = opened_metadata.len();
    let modified = opened_metadata.modified().ok();
    let modified_unix_ms = modified
        .as_ref()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .and_then(|duration| i64::try_from(duration.as_millis()).ok());

    if size_bytes > MAX_HASH_BYTES {
        let final_metadata = file.metadata().map_err(|_| {
            (
                "metadata_error".to_owned(),
                "File metadata could not be rechecked; this scan is partial.".to_owned(),
            )
        })?;
        if !same_file_state(&metadata, &opened_metadata)
            || !same_file_state(&opened_metadata, &final_metadata)
        {
            return Err((
                "changed_during_scan".to_owned(),
                "The file changed while its metadata was being inventoried; this scan is partial."
                    .to_owned(),
            ));
        }
        return Ok((
            FileRecord {
                path: relative.clone(),
                size_bytes,
                modified_unix_ms,
                sha256: None,
                hash_status: HashStatus::TooLarge,
            },
            Some(ScanWarning {
                code: "hash_size_limit".to_owned(),
                path: Some(relative),
                message: format!(
                    "File exceeds the {MAX_HASH_BYTES}-byte hash limit; metadata was inventoried without a content hash."
                ),
            }),
        ));
    }

    match hash_file(&mut file) {
        Ok(sha256) => {
            let final_metadata = file.metadata().map_err(|_| {
                (
                    "metadata_error".to_owned(),
                    "File metadata could not be rechecked after hashing; this scan is partial."
                        .to_owned(),
                )
            })?;
            let final_path = fs::canonicalize(path).map_err(|_| {
                (
                    "path_changed_during_scan".to_owned(),
                    "The file path changed while hashing; this scan is partial.".to_owned(),
                )
            })?;
            let final_path_metadata = fs::symlink_metadata(path).map_err(|_| {
                (
                    "path_changed_during_scan".to_owned(),
                    "The file path changed while hashing; this scan is partial.".to_owned(),
                )
            })?;
            if !same_file_state(&opened_metadata, &final_metadata)
                || !same_file_state(&metadata, &opened_metadata)
                || final_path != canonical
                || !final_path.starts_with(root)
                || final_path_metadata.file_type().is_symlink()
            {
                return Err((
                    "changed_during_scan".to_owned(),
                    "The file or path changed while hashing; this scan is partial.".to_owned(),
                ));
            }
            Ok((
                FileRecord {
                    path: relative,
                    size_bytes,
                    modified_unix_ms,
                    sha256: Some(sha256),
                    hash_status: HashStatus::Hashed,
                },
                None,
            ))
        }
        Err(_) => Ok((
            FileRecord {
                path: relative.clone(),
                size_bytes,
                modified_unix_ms,
                sha256: None,
                hash_status: HashStatus::ReadError,
            },
            Some(ScanWarning {
                code: "read_error".to_owned(),
                path: Some(relative),
                message: "File contents could not be read for hashing; this scan is partial."
                    .to_owned(),
            }),
        )),
    }
}

fn same_file_state(before: &fs::Metadata, after: &fs::Metadata) -> bool {
    before.len() == after.len() && before.modified().ok() == after.modified().ok()
}

fn hash_file(file: &mut File) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut remaining = MAX_HASH_BYTES + 1;
    loop {
        let limit = usize::try_from(remaining.min(buffer.len() as u64)).unwrap_or(buffer.len());
        let read = file.read(&mut buffer[..limit])?;
        if read == 0 {
            break;
        }
        remaining -= read as u64;
        hasher.update(&buffer[..read]);
        if remaining == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "file grew beyond hash limit",
            ));
        }
    }
    Ok(hex::encode(hasher.finalize()))
}

enum FileComparison {
    Unchanged,
    Changed,
    Unverified,
}

fn compare_file(previous: &StoredFile, current: &FileRecord) -> FileComparison {
    if current.hash_status == HashStatus::ReadError || previous.hash_status == HashStatus::ReadError
    {
        return FileComparison::Unverified;
    }
    if current.hash_status == HashStatus::TooLarge || previous.hash_status == HashStatus::TooLarge {
        if previous.size_bytes != current.size_bytes
            || previous.modified_unix_ms != current.modified_unix_ms
            || previous.hash_status != current.hash_status
        {
            return FileComparison::Changed;
        }
        // Metadata equality is not a content identity guarantee. Large files
        // without hashes must remain explicitly unverified between scans.
        return FileComparison::Unverified;
    }
    match (&previous.sha256, &current.sha256) {
        (Some(left), Some(right)) if left == right => FileComparison::Unchanged,
        (Some(_), Some(_)) => FileComparison::Changed,
        (None, None)
            if previous.size_bytes == current.size_bytes
                && previous.modified_unix_ms == current.modified_unix_ms
                && previous.hash_status == current.hash_status =>
        {
            FileComparison::Unchanged
        }
        _ if previous.size_bytes != current.size_bytes
            || previous.modified_unix_ms != current.modified_unix_ms
            || previous.hash_status != current.hash_status =>
        {
            FileComparison::Changed
        }
        _ => FileComparison::Unverified,
    }
}

fn to_stored(record: &FileRecord) -> StoredFile {
    StoredFile {
        path: record.path.clone(),
        size_bytes: record.size_bytes,
        modified_unix_ms: record.modified_unix_ms,
        sha256: record.sha256.clone(),
        hash_status: record.hash_status.clone(),
    }
}

fn relative_path(root: &Path, path: &Path) -> std::result::Result<String, TramuriError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| TramuriError::NonUtf8Path(path.to_path_buf()))?;
    let mut components = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(part) => components.push(
                part.to_str()
                    .ok_or_else(|| TramuriError::NonUtf8Path(path.to_path_buf()))?,
            ),
            Component::CurDir => {}
            _ => return Err(TramuriError::NonUtf8Path(path.to_path_buf())),
        }
    }
    Ok(components.join("/"))
}

fn path_to_string(path: &Path) -> Result<String> {
    path.to_str()
        .map(|value| value.replace('\\', "/"))
        .ok_or_else(|| TramuriError::NonUtf8Path(path.to_path_buf()))
}

fn warning_for(root: &Path, path: &Path, code: &str, message: &str) -> ScanWarning {
    ScanWarning {
        code: code.to_owned(),
        path: relative_path(root, path).ok(),
        message: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[cfg(windows)]
    use std::os::windows::fs::symlink_file as create_symlink_file;
    use tempfile::TempDir;

    fn initialized_project() -> TempDir {
        let temp = TempDir::new().expect("temporary directory should be created");
        init(temp.path()).expect("project should initialize");
        temp
    }

    #[cfg(unix)]
    fn read_inventory(root: &Path) -> Vec<StoredFile> {
        let connection = open_database(root).expect("database should open");
        load_inventory(&connection).expect("inventory should load")
    }

    #[test]
    fn init_creates_database_and_is_idempotent() {
        let temp = TempDir::new().expect("temporary directory should be created");
        let first = init(temp.path()).expect("first init should succeed");
        let second = init(temp.path()).expect("second init should succeed");
        assert!(!first.already_initialized);
        assert!(second.already_initialized);
        assert_eq!(first.database_path, second.database_path);
        assert_eq!(first.schema_version, 1);
    }

    #[test]
    fn empty_project_scans_as_complete() {
        let temp = initialized_project();
        let report = scan(temp.path()).expect("empty project should scan");
        assert!(report.complete);
        assert!(report.files.is_empty());
        assert!(report.deleted.is_empty());
    }

    #[test]
    fn scan_honors_gitignore_and_secret_denylists() {
        let temp = initialized_project();
        fs::write(temp.path().join(".gitignore"), "ignored.txt\nsecret-dir/\n")
            .expect("ignore file should be written");
        fs::write(temp.path().join("ignored.txt"), "ignored")
            .expect("ignored file should be written");
        fs::write(temp.path().join("visible.txt"), "visible")
            .expect("visible file should be written");
        fs::write(temp.path().join(".env"), "SECRET=not persisted")
            .expect("secret file should be written");
        fs::write(temp.path().join("service.pem"), "private key")
            .expect("key file should be written");
        fs::create_dir(temp.path().join("secret-dir")).expect("directory should be created");
        fs::write(temp.path().join("secret-dir/token.txt"), "token")
            .expect("nested secret should be written");

        let report = scan(temp.path()).expect("project should scan");
        let paths = report
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(paths, [".gitignore", "visible.txt"]);
        assert_eq!(report.sensitive_entries_excluded, 2);
        assert!(!paths.iter().any(|path| path.contains("secret")));
    }

    #[test]
    fn scan_preserves_unicode_paths_and_orders_them() {
        let temp = initialized_project();
        fs::write(temp.path().join("zeta.txt"), "z").expect("file should be written");
        fs::write(temp.path().join("café.txt"), "c").expect("unicode file should be written");
        fs::write(temp.path().join("alpha.txt"), "a").expect("file should be written");

        let report = scan(temp.path()).expect("project should scan");
        let paths = report
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(paths, ["alpha.txt", "café.txt", "zeta.txt"]);
    }

    #[test]
    fn scan_persists_added_changed_unchanged_and_deleted_files() {
        let temp = initialized_project();
        fs::write(temp.path().join("same.txt"), "same").expect("file should be written");
        fs::write(temp.path().join("change.txt"), "before").expect("file should be written");
        let first = scan(temp.path()).expect("first scan should succeed");
        assert_eq!(first.added, ["change.txt", "same.txt"]);

        fs::write(temp.path().join("change.txt"), "after!").expect("file should change");
        fs::write(temp.path().join("new.txt"), "new").expect("file should be added");
        fs::remove_file(temp.path().join("same.txt")).expect("file should be removed");
        let second = scan(temp.path()).expect("second scan should succeed");
        assert_eq!(second.added, ["new.txt"]);
        assert_eq!(second.changed, ["change.txt"]);
        assert_eq!(second.deleted, ["same.txt"]);

        let third = scan(temp.path()).expect("repeat scan should succeed");
        assert_eq!(third.unchanged, ["change.txt", "new.txt"]);
        assert!(third.added.is_empty());
        assert!(third.changed.is_empty());
        assert!(third.deleted.is_empty());
    }

    #[test]
    fn large_file_is_inventoried_without_hash() {
        let temp = initialized_project();
        let path = temp.path().join("large.bin");
        let file = File::create(&path).expect("large file should be created");
        file.set_len(MAX_HASH_BYTES + 1)
            .expect("file should be sparse-expanded");

        let report = scan(temp.path()).expect("large file should be inventoried");
        assert!(report.complete);
        assert_eq!(report.files.len(), 1);
        assert_eq!(report.files[0].hash_status, HashStatus::TooLarge);
        assert!(report.files[0].sha256.is_none());
        assert_eq!(report.warnings[0].code, "hash_size_limit");
        let next = scan(temp.path()).expect("large file should be inventoried again");
        assert_eq!(next.unverified, ["large.bin"]);
        assert!(next.unchanged.is_empty());
        assert!(next.changed.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_makes_scan_partial_and_does_not_follow_target() {
        use std::os::unix::fs::symlink;
        let temp = initialized_project();
        let outside = TempDir::new().expect("outside directory should be created");
        fs::write(outside.path().join("outside.txt"), "outside")
            .expect("outside file should be written");
        symlink(outside.path(), temp.path().join("linked-dir")).expect("symlink should be created");

        let report = scan(temp.path()).expect("project should scan partially");
        assert!(!report.complete);
        assert!(
            report
                .files
                .iter()
                .all(|file| file.path != "linked-dir/outside.txt")
        );
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.code == "symlink_skipped")
        );
    }

    #[cfg(windows)]
    #[test]
    fn symlink_makes_scan_partial_and_does_not_follow_target() {
        let temp = initialized_project();
        let outside = TempDir::new().expect("outside directory should be created");
        fs::write(outside.path().join("outside.txt"), "outside")
            .expect("outside file should be written");
        let link = temp.path().join("linked-file.txt");
        if create_symlink_file(outside.path().join("outside.txt"), &link).is_err() {
            // Windows CI may not grant symlink privileges; this environmental case is unsupported.
            return;
        }

        let report = scan(temp.path()).expect("project should scan partially");
        assert!(!report.complete);
        assert!(
            !report
                .files
                .iter()
                .any(|file| file.path == "linked-file.txt")
        );
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.code == "symlink_skipped")
        );
    }

    #[cfg(unix)]
    #[test]
    fn incomplete_scan_does_not_infer_deletions_or_prune_rows() {
        let temp = initialized_project();
        fs::write(temp.path().join("known.txt"), "keep").expect("known file should be written");
        scan(temp.path()).expect("initial scan should succeed");
        fs::remove_file(temp.path().join("known.txt")).expect("known file should be removed");

        // A dangling symlink causes an explicit partial scan.
        use std::os::unix::fs::symlink;
        symlink(
            temp.path().join("missing-target"),
            temp.path().join("dangling-link"),
        )
        .expect("dangling link should be created");
        let report = scan(temp.path()).expect("partial scan should return a report");
        assert!(!report.complete);
        assert!(report.deleted.is_empty());
        let inventory = read_inventory(temp.path());
        assert!(inventory.iter().any(|file| file.path == "known.txt"));
    }

    #[test]
    fn scan_requires_initialization() {
        let temp = TempDir::new().expect("temporary directory should be created");
        let error = scan(temp.path()).expect_err("uninitialized project should fail");
        assert!(matches!(error, TramuriError::NotInitialized(_)));
    }

    #[cfg(unix)]
    #[test]
    fn init_refuses_symlinked_state_directory_or_database() {
        use std::os::unix::fs::symlink;
        let temp = TempDir::new().expect("temporary directory should be created");
        let outside = TempDir::new().expect("outside directory should be created");
        symlink(outside.path(), temp.path().join(".tramuri"))
            .expect("state directory link should be created");
        assert!(matches!(
            init(temp.path()),
            Err(TramuriError::InvalidStatePath(_))
        ));

        let other = TempDir::new().expect("second project should be created");
        fs::create_dir(other.path().join(".tramuri")).expect("state directory should be created");
        fs::write(outside.path().join("other.sqlite3"), "not a database")
            .expect("outside file should be created");
        symlink(
            outside.path().join("other.sqlite3"),
            other.path().join(".tramuri/tramuri.sqlite3"),
        )
        .expect("database link should be created");
        assert!(matches!(
            init(other.path()),
            Err(TramuriError::InvalidStatePath(_))
        ));
    }

    #[test]
    fn scan_report_serializes_as_stable_json() {
        let temp = initialized_project();
        fs::write(temp.path().join("hello.txt"), "hello").expect("file should be written");
        let report = scan(temp.path()).expect("project should scan");
        let json = serde_json::to_value(report).expect("report should serialize");
        assert_eq!(json["complete"], true);
        assert_eq!(json["files"][0]["path"], "hello.txt");
        assert_eq!(json["files"][0]["hash_status"], "hashed");
    }
}
