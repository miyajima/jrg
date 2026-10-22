#!/usr/bin/env python3
"""Add only the missing fourth cell to the frozen two-by-two comparison."""
from datetime import datetime, timezone
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parent
BASE = ROOT.parent
sys.path.insert(0, str(BASE))
from evaluate import REPO, Support, compact, digest, verify_frozen, validate_provenance, write_json
from score import score_units


def main():
    if (ROOT / "frozen_inputs.json").exists() or (ROOT / "outputs").exists():
        raise RuntimeError("Preserve the single additional run")
    original_manifest = json.loads((BASE / "frozen_inputs.json").read_text())
    verify_frozen(original_manifest)
    previous = json.loads((BASE / "results.json").read_text())
    assert digest(BASE / "frozen_inputs.json") == previous["frozen_inputs_sha256"]
    for row in previous["rows"]:
        assert digest(BASE / row["output_file"]) == row["sha256"]
    paths = [ROOT / "PLAN.md", Path(__file__), BASE / "frozen_inputs.json", BASE / "results.json", BASE / "score.py"]
    frozen = {"frozen_at": datetime.now(timezone.utc).isoformat(),
              "added_arm": {"id": "D", "english_file": "improved_english.json", "query_rules": "off"},
              "original_frozen_inputs_sha256": digest(BASE / "frozen_inputs.json"),
              "original_results_sha256": digest(BASE / "results.json"),
              "files": {str(p.relative_to(REPO)): digest(p) for p in paths}}
    write_json(ROOT / "frozen_inputs.json", frozen)
    (ROOT / "outputs").mkdir()
    questions = json.loads((BASE / "questions.json").read_text())["questions"]
    english = {row["id"]: row["query_en"] for row in json.loads((BASE / "improved_english.json").read_text())}
    gold = {row["id"]: row["units"] for row in json.loads((BASE / "gold.json").read_text())["cases"]}
    started = datetime.now(timezone.utc).isoformat()
    rows, support = [], Support()
    try:
        for question in questions:
            name = question["id"]
            page = json.loads((BASE / f"{name}-page.json").read_text())
            original = json.loads((BASE / f"{name}-documents.json").read_text())["documents"]
            for budget in original_manifest["budgets"]:
                request = {"query": question["question_ja"], "query_expansions": [english[name]],
                           "max_tokens": budget, "input": page}
                process = subprocess.run([str(REPO / "target/debug/jrg-select"), "--query-rules", "off",
                    "--adapter", "markdown-page", "-"], input=compact(request), text=True,
                    capture_output=True, timeout=30, check=True)
                value = json.loads(process.stdout)
                validate_provenance(value["documents"], original)
                assert support.count(compact(value)) == value["tokens"] <= budget
                kept = score_units(value["documents"], gold[name])
                path = f"outputs/{name}-{budget}-D.json"
                write_json(ROOT / path, value)
                rows.append({"id": name, "budget": budget, "arm": "D", "hits": len(kept),
                    "total": len(gold[name]), "retained_units": kept, "complete": len(kept) == len(gold[name]),
                    "input_tokens": support.count(compact(page)), "output_tokens": value["tokens"],
                    "status": value["status"], "output_file": path, "sha256": digest(ROOT / path)})
                print(f"{name} {budget} D: {len(kept)}/{len(gold[name])}", flush=True)
    finally:
        support.close()
    verify_frozen(original_manifest)
    for path, expected in frozen["files"].items():
        assert digest(REPO / path) == expected
    combined = [{**r, "output_root": "base"} for r in previous["rows"]]
    combined += [{**r, "output_root": "ablation"} for r in rows]
    aggregates, contrasts = [], []
    indexed = {(r["id"], r["budget"], r["arm"]): r for r in combined}
    for budget in original_manifest["budgets"]:
        for arm in ("A", "B", "D", "C"):
            group = [r for r in combined if r["budget"] == budget and r["arm"] == arm]
            aggregates.append({"budget": budget, "arm": arm, "cases": len(group),
                **{key: sum(r[key] for r in group) for key in ("hits", "total", "input_tokens", "output_tokens")},
                "complete_cases": sum(r["complete"] for r in group)})
        for before, after in (("A", "D"), ("A", "B"), ("D", "C")):
            cases = []
            for question in questions:
                a = indexed[(question["id"], budget, before)]
                b = indexed[(question["id"], budget, after)]
                cases.append({"id": question["id"], "before_hits": a["hits"], "after_hits": b["hits"],
                    "gained": sorted(set(b["retained_units"]) - set(a["retained_units"])),
                    "lost": sorted(set(a["retained_units"]) - set(b["retained_units"]))})
            contrasts.append({"budget": budget, "before": before, "after": after, "cases": cases,
                "improved": sum(r["after_hits"] > r["before_hits"] for r in cases),
                "regressed": sum(r["after_hits"] < r["before_hits"] for r in cases),
                "unchanged": sum(r["after_hits"] == r["before_hits"] for r in cases)})
    write_json(ROOT / "results.json", {"started_at": started, "finished_at": datetime.now(timezone.utc).isoformat(),
        "frozen_inputs_sha256": digest(ROOT / "frozen_inputs.json"), "new_rows": rows,
        "combined_rows": combined, "aggregates": aggregates, "contrasts": contrasts,
        "scope": "User-requested post-hoc fourth arm; fixed English/source/gold/binary, no regeneration or tuning."})
    print(json.dumps(aggregates, indent=2))


if __name__ == "__main__":
    main()
