from __future__ import annotations

import os
from pathlib import Path
import subprocess
import venv

import pytest


def test_wheel_installs_and_sdk_imports_without_a_rust_build(tmp_path: Path) -> None:
    wheel_value = os.environ.get("TRAMURI_WHEEL")
    if not wheel_value:
        pytest.skip("Set TRAMURI_WHEEL to a built wheel to run the clean-install smoke test.")

    wheel = Path(wheel_value).resolve(strict=True)
    assert wheel.suffix == ".whl"

    environment = tmp_path / "venv"
    venv.EnvBuilder(with_pip=True).create(environment)
    python = environment / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
    subprocess.run(
        [
            str(python),
            "-m",
            "pip",
            "install",
            "--no-index",
            "--no-deps",
            str(wheel),
        ],
        check=True,
        capture_output=True,
        text=True,
    )

    project = tmp_path / "smoke-project"
    project.mkdir()
    source = project / "hello.txt"
    source.write_text("installed wheel smoke test\n", encoding="utf-8")
    smoke = """
import tramuri
from pathlib import Path
import tramuri._native
assert Path(tramuri.__file__).with_name("py.typed").is_file()
report = tramuri.init(project_path)
assert report["schema_version"] == 1
scan = tramuri.scan(project_path)
assert scan["complete"] is True
assert [item["path"] for item in scan["files"]] == ["hello.txt"]
"""
    subprocess.run(
        [str(python), "-c", f"project_path = {str(project)!r}\n{smoke}"],
        check=True,
        cwd=tmp_path,
        capture_output=True,
        text=True,
    )
