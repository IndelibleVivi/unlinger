#!/usr/bin/env python3
"""Fail-closed verification of the assembled App's executable and fixtures."""
from __future__ import annotations

import json
from pathlib import Path
import re
import subprocess
import sys

SUPPORTED_FIXTURE_SCHEMAS = frozenset({3, 4, 5})
APP_LOCAL_FIXTURE_SCHEMA = 1


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
        relative = path.relative_to(directory)
        if "schema_version" not in document:
            fixture_schema_version = document.get("fixture_schema_version")
            if (type(fixture_schema_version) is not int
                    or fixture_schema_version != APP_LOCAL_FIXTURE_SCHEMA
                    or relative.parent != Path("v3")):
                raise ValueError(f"fixture has an unsupported schema generation: {path.name}")
            continue
        schema_version = document["schema_version"]
        if type(schema_version) is not int or schema_version not in SUPPORTED_FIXTURE_SCHEMAS:
            raise ValueError(f"fixture has an unsupported schema generation: {path.name}")
        if relative.parent != Path(f"v{schema_version}"):
            raise ValueError(f"fixture schema generation does not match its directory: {path.name}")


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
