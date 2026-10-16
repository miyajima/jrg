#!/usr/bin/env python3
"""Exercise native installation without modifying the user's home directory."""
from pathlib import Path
import os
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent


class InstallTests(unittest.TestCase):
    def test_install_and_reinstall_with_spaces(self):
        with tempfile.TemporaryDirectory(prefix="jrg-install-") as tmp:
            dest = Path(tmp) / "bin with spaces"
            command = ["sh", str(ROOT / "install.sh"), "--bin-dir", str(dest)]
            subprocess.run(command, check=True, capture_output=True)
            subprocess.run(command, check=True, capture_output=True)
            result = subprocess.check_output([str(dest / "jrg-ai"), "がっこう"],
                                             input="ｶﾞｯｺｳ\n".encode()).decode()
            self.assertIn("matched=1 shown=1 omitted=0", result)
            self.assertFalse(list(dest.glob(".*-install.*")))
            self.assertEqual(subprocess.check_output([str(dest / "jrg"), "--version"]), b"jrg 0.8.0\n")

    def test_invalid_binary_does_not_replace_existing_install(self):
        with tempfile.TemporaryDirectory(prefix="jrg-install-") as tmp:
            dest = Path(tmp) / "bin"
            dest.mkdir()
            original = dest / "jrg"
            original.write_text("previous install\n")
            invalid = Path(tmp) / "invalid"
            invalid.write_text("#!/bin/sh\necho wrong\n")
            invalid.chmod(0o755)
            result = subprocess.run(["sh", str(ROOT / "install.sh"), "--bin-dir", str(dest),
                                     "--binary", str(invalid)], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(original.read_text(), "previous install\n")

    def test_native_build_fallback_uses_cargo(self):
        with tempfile.TemporaryDirectory(prefix="jrg-install-") as tmp:
            fixture = Path(tmp) / "source"
            fixture.mkdir()
            (fixture / "Cargo.toml").write_text((ROOT / "Cargo.toml").read_text())
            (fixture / "install.sh").write_text((ROOT / "install.sh").read_text())
            tools = Path(tmp) / "tools"
            tools.mkdir()
            cargo = tools / "cargo"
            cargo.write_text('''#!/bin/sh
set -eu
while [ "$#" -gt 0 ]; do
    case "$1" in
        --target-dir) out=$2; shift 2 ;;
        *) shift ;;
    esac
done
mkdir -p "$out/release"
cp "$JRG_TEST_BINARY" "$out/release/jrg"
''')
            cargo.chmod(0o755)
            env = os.environ.copy()
            env["PATH"] = str(tools) + os.pathsep + env["PATH"]
            env["JRG_TEST_BINARY"] = str(ROOT / "target/release/jrg")
            env["CARGO_TARGET_DIR"] = str(Path(tmp) / "build output")
            dest = Path(tmp) / "bin"
            result = subprocess.run(["sh", str(fixture / "install.sh"), "--bin-dir", str(dest)],
                                    env=env, capture_output=True, check=True)
            self.assertIn(b"Building jrg natively", result.stderr)
            self.assertTrue((dest / "jrg-ai").is_file())


if __name__ == "__main__":
    unittest.main()
