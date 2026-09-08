#!/usr/bin/env python3
"""Fail-closed verification of the assembled App's executable and fixtures."""
from __future__ import annotations

import json
from pathlib import Path
import re
import subprocess
import sys


def verify_executable(executable: Path) -> None:
    # Consume all output: grep -q pipelines can hide producer errors/SIGPIPE.
    commands = subprocess.run(
        ["otool", "-l", str(executable)], check=True, capture_output=True,
        text=True, errors="replace",
    ).stdout
    if re.search(r"^\s*path /Volumes/", commands, re.MULTILINE):
        raise ValueError("packaged executable retains a removable-volume LC_RPATH")
    strings = subprocess.run(
        ["strings", str(executable)], check=True, capture_output=True,
        text=True, errors="replace",
    ).stdout
    if re.search(r"/Volumes/[^\n]*UnlingerApp_UnlingerKit\.bundle", strings):
        raise ValueError("packaged resource fallback points at a removable volume")


def verify_fixtures(directory: Path) -> None:
    fixtures = sorted(directory.rglob("*.json"))
    if not directory.is_dir() or not fixtures:
        raise ValueError("packaged fixtures are missing")
    for path in fixtures:
        document = json.loads(path.read_text(encoding="utf-8"))
        if not isinstance(document, dict):
            raise ValueError(f"fixture is not an object: {path.name}")
        if document.get("schema_version") == 2:
            raise ValueError(f"stale v2 daemon fixture packaged as active: {path.name}")


def main() -> None:
    if len(sys.argv) != 2:
        raise ValueError("usage: verify_bundle.py APP_BUNDLE")
    bundle = Path(sys.argv[1])
    verify_executable(bundle / "Contents/MacOS/UnlingerApp")
    verify_fixtures(bundle / "Contents/Resources/UnlingerApp_UnlingerKit.bundle/Fixtures")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"bundle verification failed: {error}", file=sys.stderr)
        sys.exit(1)
