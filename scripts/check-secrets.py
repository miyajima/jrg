#!/usr/bin/env python3
"""Reject Google API key patterns in tracked files; never print their values."""
from pathlib import Path
import re
import subprocess
import sys


def main():
    try:
        root = Path(subprocess.check_output(
            ["git", "rev-parse", "--show-toplevel"], stderr=subprocess.DEVNULL,
        ).decode().strip())
        files = subprocess.check_output(["git", "ls-files", "-z"], cwd=root)
        pattern = re.compile(rb"AIza[0-9A-Za-z_-]{35}")
        found = False
        for raw_path in filter(None, files.split(b"\0")):
            path = Path(raw_path.decode(sys.getfilesystemencoding(), "surrogateescape"))
            full_path = root / path
            if full_path.is_symlink() or full_path.is_dir():
                continue
            content = full_path.read_bytes()
            for match in pattern.finditer(content):
                line = content.count(b"\n", 0, match.start()) + 1
                print(f"{path}:{line}: Google API key pattern detected", file=sys.stderr)
                found = True
        return 1 if found else 0
    except (OSError, subprocess.CalledProcessError) as error:
        print(f"Secret scan failed: {type(error).__name__}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
