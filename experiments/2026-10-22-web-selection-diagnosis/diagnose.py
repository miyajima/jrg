#!/usr/bin/env python3
"""Post-hoc, source-informed synonym probes; not a new held-out evaluation."""
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parent
HELDOUT = ROOT.parent / "2026-10-22-web-selection-heldout"
sys.path.insert(0, str(HELDOUT))
from evaluate import REPO, Support, compact, validate_provenance, verify_frozen, write_json
from score import score_units


def main():
    output_path = ROOT / "results.json"
    if output_path.exists():
        raise RuntimeError("Preserve existing diagnostic results")
    frozen = json.loads((HELDOUT / "frozen_inputs.json").read_text())
    verify_frozen(frozen)
    english = {x["id"]: x["query_en"] for x in json.loads((HELDOUT / "generated_english.json").read_text())}
    questions = {x["id"]: x["question_ja"] for x in json.loads((HELDOUT / "questions.json").read_text())["questions"]}
    gold = {x["id"]: x["units"] for x in json.loads((HELDOUT / "gold.json").read_text())["cases"]}
    baseline = json.loads((HELDOUT / "results.json").read_text())["rows"]
    rows, support = [], Support()
    try:
        for name, alias in (("redis", "non-positive timeout"), ("s3", "general purpose buckets")):
            page = json.loads((HELDOUT / f"{name}-page.json").read_text())
            original = json.loads((HELDOUT / f"{name}-documents.json").read_text())["documents"]
            request = {"query": questions[name], "query_expansions": [english[name], alias],
                       "max_tokens": 1024, "input": page}
            process = subprocess.run([str(REPO / "target/debug/jrg-select"), "--adapter", "markdown-page", "-"],
                                     input=compact(request), text=True, capture_output=True, check=True, timeout=30)
            result = json.loads(process.stdout)
            validate_provenance(result["documents"], original)
            assert support.count(compact(result)) == result["tokens"] <= 1024
            before = next(x for x in baseline if x["id"] == name and x["budget"] == 1024 and x["arm"] == "automatic_english")
            after = score_units(result["documents"], gold[name])
            row = {"id": name, "added_alias": alias, "budget": 1024,
                   "baseline_retained": before["retained_units"], "probe_retained": after,
                   "total": len(gold[name]), "tokens": result["tokens"], "output": result}
            rows.append(row)
            print(json.dumps({k: v for k, v in row.items() if k != "output"}), flush=True)
    finally:
        support.close()
    verify_frozen(frozen)
    write_json(output_path, {"kind": "post-hoc source-informed manual alias diagnostic",
                            "limitation": "Uses terminology observed in failed source blocks; cannot establish automatic generation or held-out generalization. Original benchmark remains unchanged.",
                            "product_changed": False, "frozen_heldout_unchanged": True, "rows": rows})


if __name__ == "__main__":
    main()
