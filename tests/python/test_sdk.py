from __future__ import annotations

import json
import os
from pathlib import Path
import shutil
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


def _cli_executable() -> str | None:
    return os.environ.get("TRAMURI_CLI")


def _copy_discovery_fixture(project_root: Path) -> None:
    fixture_root = Path(__file__).parents[1] / "fixtures" / "discovery"
    shutil.copytree(fixture_root, project_root)


def _normalize_report(report: dict[str, object]) -> dict[str, object]:
    normalized = dict(report)
    normalized.pop("project_root", None)
    files = normalized.get("files")
    if isinstance(files, list):
        normalized["files"] = [
            {key: value for key, value in entry.items() if key != "modified_unix_ms"}
            for entry in files
        ]
    return normalized


def _normalize_init_report(report: dict[str, object]) -> dict[str, object]:
    normalized = dict(report)
    normalized.pop("project_root", None)
    database_path = Path(str(normalized.pop("database_path")))
    normalized["database_location"] = database_path.parts[-2:]
    return normalized


def test_cli_and_sdk_init_json_have_normalized_parity(tmp_path: Path) -> None:
    cli = _cli_executable()
    if not cli:
        pytest.skip("Set TRAMURI_CLI to the built native executable to run CLI/SDK parity.")

    cli_project = tmp_path / "cli-project"
    sdk_project = tmp_path / "sdk-project"
    _copy_discovery_fixture(cli_project)
    _copy_discovery_fixture(sdk_project)

    sdk_report = init(sdk_project)
    cli_result = subprocess.run(
        [cli, "init", str(cli_project), "--json"],
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    cli_report = json.loads(cli_result.stdout)

    assert cli_result.stderr == ""
    assert cli_report["project_root"] != sdk_report["project_root"]
    assert _normalize_init_report(cli_report) == _normalize_init_report(sdk_report)


def test_cli_and_sdk_scan_json_have_normalized_parity(tmp_path: Path) -> None:
    cli = _cli_executable()
    if not cli:
        pytest.skip("Set TRAMURI_CLI to the built native executable to run CLI/SDK parity.")

    cli_project = tmp_path / "cli-project"
    sdk_project = tmp_path / "sdk-project"
    for project in (cli_project, sdk_project):
        _copy_discovery_fixture(project)

    init(sdk_project)
    subprocess.run(
        [cli, "init", str(cli_project), "--json"],
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    sdk_report = scan(sdk_project)
    cli_result = subprocess.run(
        [cli, "scan", str(cli_project), "--json"],
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    cli_report = json.loads(cli_result.stdout)

    assert cli_result.stderr == ""
    assert cli_report["project_root"] != sdk_report["project_root"]
    assert _normalize_report(cli_report) == _normalize_report(sdk_report)
    assert "ignored/not-indexed.txt" not in {entry["path"] for entry in cli_report["files"]}
    assert "sensitive/.env.test" not in {entry["path"] for entry in cli_report["files"]}
    assert cli_report["sensitive_entries_excluded"] == 1


def test_cli_and_sdk_uninitialized_scan_errors_are_equivalent(tmp_path: Path) -> None:
    cli = _cli_executable()
    if not cli:
        pytest.skip("Set TRAMURI_CLI to the built native executable to run CLI/SDK parity.")

    project = tmp_path / "uninitialized-project"
    project.mkdir()
    cli_result = subprocess.run(
        [cli, "scan", str(project), "--json"],
        capture_output=True,
        text=True,
        encoding="utf-8",
    )

    with pytest.raises(TramuriError, match="init"):
        scan(project)

    assert cli_result.returncode != 0
    assert cli_result.stdout == ""
    assert "init" in cli_result.stderr


def test_cli_and_sdk_partial_scan_have_normalized_parity(tmp_path: Path) -> None:
    cli = _cli_executable()
    if not cli:
        pytest.skip("Set TRAMURI_CLI to the built native executable to run CLI/SDK parity.")

    outside = tmp_path / "outside.txt"
    outside.write_text("outside the project", encoding="utf-8")
    cli_project = tmp_path / "cli-partial"
    sdk_project = tmp_path / "sdk-partial"
    for project in (cli_project, sdk_project):
        _copy_discovery_fixture(project)
        try:
            (project / "external-link.txt").symlink_to(outside)
        except OSError as error:
            pytest.skip(f"Host cannot create symlinks required for partial-scan parity: {error}")
        init(project)

    sdk_report = scan(sdk_project)
    cli_result = subprocess.run(
        [cli, "scan", str(cli_project), "--json"],
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    cli_report = json.loads(cli_result.stdout)

    assert cli_report["complete"] is False
    assert sdk_report["complete"] is False
    assert _normalize_report(cli_report) == _normalize_report(sdk_report)
