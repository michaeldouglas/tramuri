use std::fs;
use std::process::Command;

use serde_json::Value;
use tempfile::tempdir;

fn tramuri() -> Command {
    Command::new(env!("CARGO_BIN_EXE_tramuri"))
}

#[test]
fn help_is_available() {
    let output = tramuri().arg("--help").output().expect("CLI should launch");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("help should be UTF-8");
    assert!(stdout.contains("Usage: tramuri"));
    assert!(stdout.contains("init"));
    assert!(stdout.contains("scan"));
}

#[test]
fn init_and_scan_emit_machine_readable_json() {
    let project = tempdir().expect("project directory should be created");
    fs::write(project.path().join("hello.txt"), "hello\n").expect("fixture should be written");

    let init = tramuri()
        .args(["init", project.path().to_str().expect("path should be UTF-8"), "--json"])
        .output()
        .expect("CLI should launch");
    assert!(init.status.success());
    let init_json: Value = serde_json::from_slice(&init.stdout).expect("init should emit JSON");
    assert_eq!(init_json["schema_version"], 1);
    assert_eq!(init_json["already_initialized"], false);

    let scan = tramuri()
        .args(["scan", project.path().to_str().expect("path should be UTF-8"), "--json"])
        .output()
        .expect("CLI should launch");
    assert!(scan.status.success());
    assert!(scan.stderr.is_empty());
    let scan_json: Value = serde_json::from_slice(&scan.stdout).expect("scan should emit JSON");
    assert_eq!(scan_json["complete"], true);
    assert_eq!(scan_json["files"][0]["path"], "hello.txt");
    assert_eq!(scan_json["added"][0], "hello.txt");
}

#[test]
fn operational_error_is_nonzero_and_written_to_stderr() {
    let project = tempdir().expect("project directory should be created");
    let output = tramuri()
        .args(["scan", project.path().to_str().expect("path should be UTF-8"), "--json"])
        .output()
        .expect("CLI should launch");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("error should be UTF-8");
    assert!(stderr.contains("tramuri init"));
}
