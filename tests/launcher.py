#!/usr/bin/env python3
"""Behavioral tests for the local MCP launcher without starting an embedding model."""

from pathlib import Path
import os
import subprocess
import tempfile
import time
import unittest


ROOT = Path(__file__).resolve().parents[1]
LAUNCHER = ROOT / "scripts" / "jrg-mcp.sh"
STUB = """#!/bin/sh
set -eu
printf '%s\\n' "$PWD" > "$CAPTURE_DIR/pwd"
pwd -P > "$CAPTURE_DIR/physical_pwd"
printf '%s\\n' "$@" > "$CAPTURE_DIR/argv"
while [ "$#" -gt 0 ]; do
    case "$1" in
        --root)
            shift
            printf '%s' "$1" > "$CAPTURE_DIR/root"
            ;;
        --session)
            shift
            printf '%s' "$1" > "$CAPTURE_DIR/session"
            ;;
    esac
    shift
done
if [ "${JRG_TEST_BLOCK:-0}" = "1" ]; then
    while [ ! -e "$CAPTURE_DIR/release" ]; do sleep 0.02; done
fi
"""


class LauncherTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="jrg-launcher-")
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.home = self.base / "fake home"
        self.bin_dir = self.home / ".local" / "bin"
        self.bin_dir.mkdir(parents=True)
        self.tmpdir = self.base / "temporary files"
        self.tmpdir.mkdir()
        self.capture_root = self.base / "captures"
        self.capture_root.mkdir()
        self.stub = self.bin_dir / "jrg"
        self.stub.write_text(STUB, encoding="utf-8")
        self.stub.chmod(0o755)

    def environment(self, capture_dir, **overrides):
        env = os.environ.copy()
        env.update(
            {
                "HOME": str(self.home),
                "TMPDIR": str(self.tmpdir),
                "CAPTURE_DIR": str(capture_dir),
                "JRG_EMBEDDING_ENABLED": "0",
            }
        )
        env.pop("JRG_EMBEDDING_SOCKET", None)
        env.update(overrides)
        return env

    def launch(self, cwd, capture_dir, *arguments, env_overrides=None):
        capture_dir.mkdir(parents=True)
        result = subprocess.run(
            ["/bin/sh", str(LAUNCHER), *map(str, arguments)],
            cwd=cwd,
            env=self.environment(capture_dir, **(env_overrides or {})),
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            timeout=10,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        return capture_dir

    def wait_for_capture(self, process, capture_dir):
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            if (capture_dir / "session").is_file():
                return
            if process.poll() is not None:
                stdout, stderr = process.communicate(timeout=1)
                self.fail(f"launcher exited before capture: {stdout!r} {stderr!r}")
            time.sleep(0.02)
        self.fail("launcher did not reach the executable stub in time")

    def start_blocked(self, cwd, capture_dir):
        capture_dir.mkdir(parents=True)
        env = self.environment(capture_dir, JRG_TEST_BLOCK="1")
        return subprocess.Popen(
            ["/bin/sh", str(LAUNCHER), str(cwd)],
            cwd=cwd,
            env=env,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )

    def test_omitted_root_is_each_physical_launch_directory(self):
        projects = self.base / "projects space"
        project = projects / "jrg"
        source = project / "src"
        source.mkdir(parents=True)
        (project / ".git").mkdir()

        for name, cwd in (("parent", projects), ("project", project), ("child", source)):
            with self.subTest(cwd=name):
                capture = self.launch(cwd, self.capture_root / name)
                expected = str(cwd.resolve())
                args = (capture / "argv").read_text(encoding="utf-8").splitlines()
                self.assertEqual((capture / "root").read_text(encoding="utf-8"), expected)
                self.assertEqual(args[args.index("--root") + 1], expected)
                self.assertEqual((capture / "physical_pwd").read_text().strip(), expected)

    def test_explicit_root_with_spaces_overrides_launch_directory(self):
        cwd = self.base / "elsewhere"
        override = self.base / "chosen root with spaces"
        cwd.mkdir()
        override.mkdir()

        capture = self.launch(cwd, self.capture_root / "explicit", override)
        args = (capture / "argv").read_text(encoding="utf-8").splitlines()
        self.assertEqual((capture / "root").read_text(encoding="utf-8"), str(override))
        self.assertEqual(args[args.index("--root") + 1], str(override))

    def test_omitted_root_resolves_symlinked_launch_directory_physically(self):
        physical = self.base / "physical workspace"
        physical.mkdir()
        alias = self.base / "workspace alias"
        alias.symlink_to(physical, target_is_directory=True)

        capture = self.launch(
            alias,
            self.capture_root / "symlink",
            env_overrides={"PWD": str(alias)},
        )
        expected = str(physical.resolve())
        self.assertEqual((capture / "root").read_text(encoding="utf-8"), expected)
        self.assertEqual((capture / "physical_pwd").read_text().strip(), expected)

    def test_each_connection_gets_a_distinct_temporary_session_and_cleans_it(self):
        cwd = self.base / "session workspace"
        cwd.mkdir()
        captures = [self.capture_root / "connection-a", self.capture_root / "connection-b"]
        processes = []
        sessions = []
        try:
            for capture in captures:
                process = self.start_blocked(cwd, capture)
                processes.append(process)
                self.wait_for_capture(process, capture)
                sessions.append(Path((capture / "session").read_text(encoding="utf-8")))

            self.assertNotEqual(sessions[0], sessions[1])
            self.assertTrue(all(path.parent.is_dir() for path in sessions))
        finally:
            for capture in captures:
                if capture.is_dir():
                    (capture / "release").touch()
            for process in processes:
                process.communicate(timeout=10)

        self.assertTrue(all(not path.parent.exists() for path in sessions))


if __name__ == "__main__":
    unittest.main()
