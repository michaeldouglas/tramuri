"""Python SDK for Tramuri's shared Rust project-discovery core."""

from __future__ import annotations

from importlib.metadata import version
import json
import os
from typing import Any

from ._native import TramuriError, _init_json, _scan_json

PathLike = str | os.PathLike[str]


def init(path: PathLike = ".") -> dict[str, Any]:
    """Initialize Tramuri's local database in *path* and return its report."""
    encoded_path = os.fsdecode(os.fspath(path))
    return json.loads(_init_json(encoded_path))


def scan(path: PathLike = ".") -> dict[str, Any]:
    """Inventory *path* using the Rust core and return a normalized report."""
    encoded_path = os.fsdecode(os.fspath(path))
    return json.loads(_scan_json(encoded_path))


__version__ = version("tramuri")

__all__ = ["TramuriError", "__version__", "init", "scan"]
