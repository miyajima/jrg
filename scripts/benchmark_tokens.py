#!/usr/bin/env python3
"""Measure stdout tokens, not character/byte estimates, using o200k_base."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", default="target/release/jrg")
    parser.add_argument("--counter", default="target/release/examples/token_count")
    args = parser.parse_args()
    binary = str(Path(args.binary).resolve(strict=True))
    counter = str(Path(args.counter).resolve(strict=True))

    def count(payload):
        return int(subprocess.check_output([counter], input=payload))

    def search(root, extra):
        return subprocess.check_output([binary, *extra, "needle", "."], cwd=root)

    rows = []
    with tempfile.TemporaryDirectory(prefix="jrg-tokens-") as tmp:
        root = Path(tmp)
        cases = [
            ("shared_paths", "path grouping, all lines retained", 20000,
             "".join(f"needle connection_pool setting_{n} = {n}\n" for n in range(160))),
            ("duplicate_lines", "exact duplicate grouping, all locations retained", 20000,
             "needle: 接続を再試行します。retry_after=100ms\n" * 200),
            ("long_japanese_lines", "match-centered excerpts; source tails omitted", 20000,
             "".join("背景" * 400 + f"needle 認証エラー code={n}; " + "詳細" * 400 + "\n" for n in range(12))),
            ("budgeted_results", "512-token budget; some matching lines omitted", 512,
             "".join(f"needle 検索設定 key_{n} の意味を説明します。value={n}\n" for n in range(300))),
        ]
        for name, method, budget, source in cases:
            folder = root / name
            path = folder / "docs/architecture/decisions/connection-pool/retry-and-timeout-configuration.md"
            path.parent.mkdir(parents=True)
            path.write_text(source, encoding="utf-8")
            plain = search(folder, [])
            ai = search(folder, ["--ai", "--max-tokens", str(budget)])
            plain_tokens = count(plain)
            ai_tokens = count(ai)
            assert ai_tokens <= budget
            footer = ai.decode().splitlines()[-1]
            fields = {key: int(value) for key, value in re.findall(r"(\w+)=(\d+)", footer)}
            rows.append({
                "case": name, "method": method, "budget": budget,
                "plain_tokens": plain_tokens, "ai_tokens": ai_tokens,
                "reduction_percent": round(100 * (1 - ai_tokens / plain_tokens), 2),
                **fields,
            })
        print(json.dumps({
            "version": subprocess.check_output([binary, "--version"], text=True).strip(),
            "tokenizer": "o200k_base, encode_ordinary, complete stdout including footer",
            "cases": rows,
        }, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
