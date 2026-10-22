#!/usr/bin/env python3
"""Exercise the tracked-file scanner without storing real credentials."""
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


SCANNER = Path(__file__).resolve().parents[1] / "scripts/check-secrets.py"


class SecretScanTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        subprocess.run(["git", "init", "-q", str(self.root)], check=True)

    def scan(self):
        return subprocess.run(
            [sys.executable, str(SCANNER)], cwd=self.root,
            capture_output=True, text=True,
        )

    def test_detects_html_key_and_never_prints_its_value(self):
        key = "AI" + "za" + "a" * 35
        path = self.root / "page with spaces.html"
        path.write_text("<script>\nkey='" + key + "'\n</script>\n")
        subprocess.run(["git", "add", path.name], cwd=self.root, check=True)
        result = self.scan()
        self.assertEqual(result.returncode, 1)
        self.assertIn("page with spaces.html:2", result.stderr)
        self.assertNotIn(key, result.stdout + result.stderr)

    def test_clean_tracked_files_and_untracked_data(self):
        (self.root / "clean.txt").write_text("[REDACTED_GOOGLE_API_KEY]\n")
        (self.root / "untracked.html").write_text("AI" + "za" + "a" * 35)
        subprocess.run(["git", "add", "clean.txt"], cwd=self.root, check=True)
        result = self.scan()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_non_repository_is_an_error(self):
        with tempfile.TemporaryDirectory() as other:
            result = subprocess.run([sys.executable, str(SCANNER)], cwd=other,
                                    capture_output=True, text=True)
        self.assertEqual(result.returncode, 2)


if __name__ == "__main__":
    unittest.main()
