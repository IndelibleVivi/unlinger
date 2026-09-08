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

    def test_fixture_scan_rejects_whitespace_v2_and_malformed_json(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fixture = root / "sample.json"
            for content in ('{ "schema_version" : 2 }', '{broken', '[]'):
                fixture.write_text(content)
                with self.subTest(content=content), self.assertRaises(ValueError):
                    verify_fixtures(root)
            fixture.write_text(json.dumps({"schema_version": 5}))
            verify_fixtures(root)

    def test_missing_or_empty_fixture_directory_fails(self):
        with tempfile.TemporaryDirectory() as temporary:
            for root in (Path(temporary), Path(temporary) / "missing"):
                with self.assertRaises(ValueError):
                    verify_fixtures(root)


if __name__ == "__main__":
    unittest.main()
