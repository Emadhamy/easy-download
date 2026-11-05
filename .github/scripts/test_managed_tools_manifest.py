"""Unit tests for the managed tools manifest schema.

Run with: python -m unittest discover -s .github/scripts -p 'test_*.py'
"""
import copy
import json
import unittest
from pathlib import Path

from jsonschema import Draft202012Validator, FormatChecker

ROOT = Path(__file__).resolve().parents[2]
MANIFEST_PATH = ROOT / "rust/resources/managed-tools/manifest.json"
SCHEMA_PATH = ROOT / "rust/resources/managed-tools/manifest.schema.json"


def load_validator_and_manifest():
    manifest = json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))
    schema = json.loads(SCHEMA_PATH.read_text(encoding="utf-8"))
    Draft202012Validator.check_schema(schema)
    return Draft202012Validator(schema, format_checker=FormatChecker()), manifest


class ManagedToolsManifestTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.validator, cls.manifest = load_validator_and_manifest()

    def valid_release(self):
        return {
            "version": "test-version",
            "url": "https://example.invalid/tool",
            "archiveFormat": "binary",
            "sha256": "a" * 64,
            "executablePath": "tool",
        }

    def test_current_manifest_is_valid(self):
        self.assertEqual(list(self.validator.iter_errors(self.manifest)), [])

    def test_enabled_tool_without_release_is_rejected(self):
        candidate = copy.deepcopy(self.manifest)
        candidate["tools"]["ytDlp"]["enabled"] = True
        candidate["tools"]["ytDlp"]["releases"] = {}
        self.assertTrue(list(self.validator.iter_errors(candidate)))

    def test_invalid_sha256_is_rejected(self):
        candidate = copy.deepcopy(self.manifest)
        candidate["tools"]["ytDlp"]["enabled"] = True
        candidate["tools"]["ytDlp"]["releases"] = {
            "linux-x86_64": self.valid_release()
        }
        candidate["tools"]["ytDlp"]["releases"]["linux-x86_64"]["sha256"] = "z" * 64
        self.assertTrue(list(self.validator.iter_errors(candidate)))

    def test_incomplete_release_is_rejected(self):
        candidate = copy.deepcopy(self.manifest)
        candidate["tools"]["ytDlp"]["enabled"] = True
        release = self.valid_release()
        del release["version"]
        candidate["tools"]["ytDlp"]["releases"] = {"linux-x86_64": release}
        self.assertTrue(list(self.validator.iter_errors(candidate)))

    def test_valid_release_is_accepted(self):
        candidate = copy.deepcopy(self.manifest)
        candidate["tools"]["ytDlp"]["enabled"] = True
        candidate["tools"]["ytDlp"]["releases"] = {
            "linux-x86_64": self.valid_release()
        }
        self.assertEqual(list(self.validator.iter_errors(candidate)), [])


if __name__ == "__main__":
    unittest.main()
