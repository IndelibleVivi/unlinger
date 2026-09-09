#!/usr/bin/env python3
"""Synthetic verifier tests: no real App, service or process table access."""
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from verify_bundle import verify_executable, verify_fixtures


class BundleVerificationTests(unittest.TestCase):
    def test_missing_tool_and_producer_failure_are_not_clean_results(self):
        for failure in (FileNotFoundError("otool"), subprocess.CalledProcessError(1, "otool")):
            with self.subTest(failure=type(failure).__name__), patch("verify_bundle.subprocess.run", side_effect=failure):
                with self.assertRaises(type(failure)):
                    verify_executable(Path("synthetic-app"))

    def test_safe_executable_consumes_both_checked_outputs(self):
        result = subprocess.CompletedProcess([], 0, stdout="safe")
        with patch("verify_bundle.subprocess.run", return_value=result) as run:
            verify_executable(Path("synthetic-app"))
            self.assertEqual(run.call_count, 2)
            self.assertTrue(all(call.kwargs["check"] for call in run.call_args_list))

    def test_removable_rpath_and_resource_fallback_are_rejected(self):
        def output(text):
            return subprocess.CompletedProcess([], 0, stdout=text)
        for outputs in ([output("   path /Volumes/Toolchain/lib (offset 12)")],
                        [output("safe"), output("/Volumes/source/UnlingerApp_UnlingerKit.bundle")]):
            with self.subTest(outputs=outputs), patch("verify_bundle.subprocess.run", side_effect=outputs):
                with self.assertRaises(ValueError):
                    verify_executable(Path("synthetic-app"))

    def test_fixture_scan_rejects_invalid_schema_generation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fixture = root / "v3" / "sample.json"
            fixture.parent.mkdir()
            for content in (
                '{ "schema_version" : 2 }',
                '{}',
                '{"schema_version": "2"}',
                '{"schema_version": true}',
                '{"schema_version": 6}',
                '{broken',
                '[]',
            ):
                fixture.write_text(content)
                with self.subTest(content=content), self.assertRaises(ValueError):
                    verify_fixtures(root)

    def test_fixture_schema_must_match_its_generation_directory(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fixture = root / "v3" / "sample.json"
            fixture.parent.mkdir()
            fixture.write_text(json.dumps({"schema_version": 5}))
            with self.assertRaises(ValueError):
                verify_fixtures(root)

    def test_supported_fixture_generations_are_accepted(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for generation in (3, 4, 5):
                directory = root / f"v{generation}"
                directory.mkdir()
                (directory / "sample.json").write_text(
                    json.dumps({"schema_version": generation})
                )
            verify_fixtures(root)

    def test_v3_app_local_fixture_schema_is_accepted(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            directory = root / "v3"
            directory.mkdir()
            (directory / "app-scenario.json").write_text(
                json.dumps({"fixture_schema_version": 1})
            )
            verify_fixtures(root)

    def test_app_local_fixture_schema_is_exact_and_v3_only(self):
        for generation, fixture_schema_version in (
            (3, "1"),
            (3, True),
            (3, 2),
            (4, 1),
        ):
            with self.subTest(
                generation=generation,
                fixture_schema_version=fixture_schema_version,
            ), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                directory = root / f"v{generation}"
                directory.mkdir()
                (directory / "app-scenario.json").write_text(
                    json.dumps({"fixture_schema_version": fixture_schema_version})
                )
                with self.assertRaises(ValueError):
                    verify_fixtures(root)

    def test_missing_or_empty_fixture_directory_fails(self):
        with tempfile.TemporaryDirectory() as temporary:
            for root in (Path(temporary), Path(temporary) / "missing"):
                with self.assertRaises(ValueError):
                    verify_fixtures(root)


if __name__ == "__main__":
    unittest.main()
