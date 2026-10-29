from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess

import pytest

from tramuri import TramuriError, init, scan


def test_init_is_idempotent_and_scan_returns_inventory(tmp_path: Path) -> None:
    source = tmp_path / "src" / "mod.rs"
    source.parent.mkdir()
    source.write_text("pub fn sample() {}\n", encoding="utf-8")

    first_init = init(tmp_path)
    second_init = init(tmp_path)
    report = scan(tmp_path)

    assert first_init["schema_version"] == 1
    assert first_init["already_initialized"] is False
    assert second_init["already_initialized"] is True
    assert report["complete"] is True
    assert [entry["path"] for entry in report["files"]] == ["src/mod.rs"]
    assert report["added"] == ["src/mod.rs"]


def test_scan_requires_init(tmp_path: Path) -> None:
    with pytest.raises(TramuriError, match="init"):
        scan(tmp_path)


def test_cli_and_sdk_scan_json_have_normalized_parity(tmp_path: Path) -> None:
    cli = os.environ.get("TRAMURI_CLI")
    if not cli:
        pytest.skip("Set TRAMURI_CLI to the built native executable to run CLI/SDK parity.")

    cli_project = tmp_path / "cli-project"
    sdk_project = tmp_path / "sdk-project"
    for project in (cli_project, sdk_project):
        source = project / "src" / "main.rs"
        source.parent.mkdir(parents=True)
        source.write_text("fn main() {}\n", encoding="utf-8")
        (project / ".gitignore").write_text("ignored/\n", encoding="utf-8")
        (project / "ignored").mkdir()
        (project / "ignored" / "not-indexed.txt").write_text("ignored\n", encoding="utf-8")

    init(sdk_project)
    subprocess.run([cli, "init", str(cli_project), "--json"], check=True, capture_output=True, text=True)
    sdk_report = scan(sdk_project)
    cli_result = subprocess.run(
        [cli, "scan", str(cli_project), "--json"], check=True, capture_output=True, text=True
    )
    cli_report = json.loads(cli_result.stdout)

    assert cli_result.stderr == ""
    assert cli_report["project_root"] != sdk_report["project_root"]
    cli_report.pop("project_root")
    sdk_report.pop("project_root")
    for report in (cli_report, sdk_report):
        for entry in report["files"]:
            entry.pop("modified_unix_ms")
    assert cli_report == sdk_report
