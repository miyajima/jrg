#!/usr/bin/env python3
"""Verify all four cells from saved outputs; never rerun selection."""
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parent
BASE = ROOT.parent
sys.path.insert(0, str(BASE))
from evaluate import REPO, Support, compact, digest, verify_frozen, validate_provenance, write_json
from score import score_units


def main():
    base_manifest = json.loads((BASE / "frozen_inputs.json").read_text())
    verify_frozen(base_manifest)
    extra_manifest = json.loads((ROOT / "frozen_inputs.json").read_text())
    for path, expected in extra_manifest["files"].items():
        assert digest(REPO / path) == expected
    original = json.loads((BASE / "results.json").read_text())
    result = json.loads((ROOT / "results.json").read_text())
    assert digest(ROOT / "frozen_inputs.json") == result["frozen_inputs_sha256"]
    assert extra_manifest["frozen_at"] < result["started_at"] < result["finished_at"]
    assert digest(BASE / "results.json") == extra_manifest["original_results_sha256"]
    assert digest(BASE / "frozen_inputs.json") == extra_manifest["original_frozen_inputs_sha256"]
    questions = json.loads((BASE / "questions.json").read_text())["questions"]
    gold = {r["id"]: r["units"] for r in json.loads((BASE / "gold.json").read_text())["cases"]}
    combined = result["combined_rows"]
    indexed = {(r["id"], r["budget"], r["arm"]): r for r in combined}
    expected = {(q["id"], b, arm) for q in questions for b in base_manifest["budgets"] for arm in ("A", "B", "C", "D")}
    assert set(indexed) == expected and len(combined) == 64
    assert len(result["new_rows"]) == len(list((ROOT / "outputs").glob("*.json"))) == 16
    assert [{k: v for k, v in r.items() if k != "output_root"} for r in combined if r["arm"] != "D"] == original["rows"]
    assert [{k: v for k, v in r.items() if k != "output_root"} for r in combined if r["arm"] == "D"] == result["new_rows"]
    support = Support()
    try:
        for (name, budget, arm), row in indexed.items():
            root = ROOT if arm == "D" else BASE
            path = root / row["output_file"]
            assert digest(path) == row["sha256"]
            value = json.loads(path.read_text())
            documents = json.loads((BASE / f"{name}-documents.json").read_text())["documents"]
            validate_provenance(value["documents"], documents)
            assert support.count(compact(value)) == value["tokens"] == row["output_tokens"] <= budget
            assert value["max_tokens"] == budget and value["status"] == row["status"]
            retained = score_units(value["documents"], gold[name])
            assert retained == row["retained_units"] and len(retained) == row["hits"]
            assert row["total"] == len(gold[name])
            assert row["complete"] == (len(retained) == len(gold[name]))
        for aggregate in result["aggregates"]:
            group = [r for r in combined if r["budget"] == aggregate["budget"] and r["arm"] == aggregate["arm"]]
            assert aggregate["cases"] == len(group) == 8
            for field in ("hits", "total", "input_tokens", "output_tokens"):
                assert aggregate[field] == sum(r[field] for r in group)
            assert aggregate["complete_cases"] == sum(r["complete"] for r in group)
        for contrast in result["contrasts"]:
            cases = []
            for question in questions:
                a = indexed[(question["id"], contrast["budget"], contrast["before"])]
                b = indexed[(question["id"], contrast["budget"], contrast["after"])]
                cases.append({"id": question["id"], "before_hits": a["hits"], "after_hits": b["hits"],
                    "gained": sorted(set(b["retained_units"]) - set(a["retained_units"])),
                    "lost": sorted(set(a["retained_units"]) - set(b["retained_units"]))})
            assert cases == contrast["cases"]
            assert contrast["improved"] == sum(r["after_hits"] > r["before_hits"] for r in cases)
            assert contrast["regressed"] == sum(r["after_hits"] < r["before_hits"] for r in cases)
            assert contrast["unchanged"] == sum(r["after_hits"] == r["before_hits"] for r in cases)
    finally:
        support.close()
    write_json(ROOT / "verification.json", {"status": "pass", "saved_outputs_checked": 64,
        "new_outputs": 16, "original_frozen_files_checked": len(base_manifest["files"]),
        "original_results_unchanged": True, "source_and_dependency_check": "pass",
        "token_budget_and_scores": "pass", "contrasts_recomputed": "pass",
        "selector_rerun": False, "network_used": False, "results_sha256": digest(ROOT / "results.json")})
    print("PASS: 64 saved outputs, 16 new conditions, frozen originals unchanged")


if __name__ == "__main__":
    main()
