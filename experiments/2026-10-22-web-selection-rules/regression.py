#!/usr/bin/env python3
"""Known eight cases are regression data, never fresh held-out quality evidence."""
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parent
OLD = ROOT.parent / "2026-10-22-web-selection-heldout"
sys.path.insert(0, str(OLD))
from evaluate import REPO, Support, compact, validate_provenance, write_json
from score import score_units


def main():
    if (ROOT / "regression_results.json").exists():
        raise RuntimeError("Do not overwrite existing regression")
    questions = json.loads((OLD / "questions.json").read_text())["questions"]
    english = {x["id"]: x["query_en"] for x in json.loads((OLD / "generated_english.json").read_text())}
    gold = {x["id"]: x["units"] for x in json.loads((OLD / "gold.json").read_text())["cases"]}
    support, rows = Support(), []
    try:
        for question in questions:
            name = question["id"]
            page = json.loads((OLD / f"{name}-page.json").read_text())
            original = json.loads((OLD / f"{name}-documents.json").read_text())["documents"]
            request = {"query": question["question_ja"], "query_expansions": [english[name]],
                       "max_tokens": 1024, "input": page}
            results = {}
            for mode in ("off", "builtin"):
                process = subprocess.run([str(REPO / "target/debug/jrg-select"), "--query-rules", mode,
                                          "--adapter", "markdown-page", "-"], input=compact(request),
                                         capture_output=True, text=True, timeout=30, check=True)
                value = json.loads(process.stdout)
                validate_provenance(value["documents"], original)
                assert support.count(compact(value)) == value["tokens"] <= 1024
                results[mode] = value
            saved = json.loads((OLD / "outputs" / f"{name}-1024-automatic_english.json").read_text())
            assert results["off"] == saved
            before, after = (score_units(results[mode]["documents"], gold[name]) for mode in ("off", "builtin"))
            rows.append({"id": name, "off_matches_saved_legacy": True,
                         "before": before, "after": after, "total": len(gold[name]),
                         "outputs": results})
            print(name, before, "->", after, flush=True)
    finally:
        support.close()
    write_json(ROOT / "regression_results.json", {"kind": "Known regression cases; excluded from held-out aggregate", "rows": rows})


if __name__ == "__main__":
    main()
