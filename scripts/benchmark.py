#!/usr/bin/env python3
"""Generate a temporary corpus; compare warm-cache search latency with rg."""
import argparse
import json
import platform
from pathlib import Path
import shutil
import statistics
import subprocess
import tempfile
import time


def measure(command, repeats):
    subprocess.run(command, check=True, stdout=subprocess.DEVNULL)
    timings = []
    for _ in range(repeats):
        start = time.perf_counter()
        subprocess.run(command, check=True, stdout=subprocess.DEVNULL)
        timings.append((time.perf_counter() - start) * 1000)
    return round(statistics.median(timings), 2)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", default="target/release/jrg")
    parser.add_argument("--files", type=int, default=400)
    parser.add_argument("--lines", type=int, default=400)
    parser.add_argument("--repeats", type=int, default=5)
    parser.add_argument("--threads", type=int, default=4)
    args = parser.parse_args()
    if min(args.files, args.lines, args.repeats, args.threads) < 1:
        parser.error("files, lines, repeats and threads must be positive")
    binary = str(Path(args.binary).resolve(strict=True))
    with tempfile.TemporaryDirectory(prefix="jrg-bench-") as tmp:
        root = Path(tmp)
        ascii_dir = root / "ascii"
        jp_dir = root / "japanese"
        ascii_dir.mkdir()
        jp_dir.mkdir()
        ascii_line = "request accepted: status=200 method=GET route=/api/search user=sample\n"
        jp_line = "検索処理を実行しました。正常に終了しました。表記を確認します。\n"
        for n in range(args.files):
            (ascii_dir / f"{n}.txt").write_text(ascii_line * args.lines + "needle\n", encoding="utf-8")
            (jp_dir / f"{n}.txt").write_text(jp_line * args.lines + "ガッコウ\nｶﾞｯｺｳ\nがっこう\nか\u3099っこう\n", encoding="utf-8")
        result = {
            "method": "warm cache, median subprocess wall-clock ms, output discarded, -c",
            "files_per_corpus": args.files,
            "lines_per_file_before_matches": args.lines,
            "repeats": args.repeats,
            "threads": args.threads,
            "platform": platform.platform(),
            "versions": {},
            "bytes": {name: sum(p.stat().st_size for p in folder.iterdir()) for name, folder in [("ascii", ascii_dir), ("japanese", jp_dir)]},
            "latency_ms": {},
            "japanese_recall_lines": {},
        }
        tools = [("jrg", binary)]
        if shutil.which("rg"):
            tools.append(("rg", shutil.which("rg")))
        for name, executable in tools:
            result["versions"][name] = subprocess.check_output([executable, "--version"], text=True).splitlines()[0]
            for corpus, pattern, folder in [("ascii", "needle", ascii_dir), ("japanese", "がっこう", jp_dir)]:
                command = [executable, "-F", "-c", "-j", str(args.threads), pattern, str(folder)]
                result["latency_ms"][f"{name}_{corpus}"] = measure(command, args.repeats)
                if corpus == "japanese":
                    output = subprocess.check_output(command, text=True)
                    result["japanese_recall_lines"][name] = sum(int(line.rsplit(":", 1)[1]) for line in output.splitlines())
        print(json.dumps(result, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
