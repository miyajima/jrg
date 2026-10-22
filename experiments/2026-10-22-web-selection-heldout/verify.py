#!/usr/bin/env python3
"""Validate saved artifacts without rerunning selection, generation, or fetching."""
import json

from evaluate import ROOT, Support, compact, digest, verify_frozen, validate_provenance, write_json
from score import score_units


def main():
    frozen = json.loads((ROOT / "frozen_inputs.json").read_text())
    verify_frozen(frozen)
    results = json.loads((ROOT / "results.json").read_text())
    assert results["frozen_inputs_sha256"] == digest(ROOT / "frozen_inputs.json")
    assert frozen["frozen_at"] < results["started_at"] < results["finished_at"]
    questions = json.loads((ROOT / "questions.json").read_text())["questions"]
    gold = {case["id"]: case["units"] for case in json.loads((ROOT / "gold.json").read_text())["cases"]}
    keys = {(q["id"], budget, arm) for q in questions for budget in frozen["budgets"] for arm in frozen["arms"]}
    rows = {(row["id"], row["budget"], row["arm"]): row for row in results["rows"]}
    assert set(rows) == keys and len(results["rows"]) == len(keys) == 48
    assert len(list((ROOT / "outputs").glob("*.json"))) == 48
    support = Support()
    try:
        for (name, budget, arm), row in rows.items():
            path = ROOT / row["output_file"]
            assert digest(path) == row["sha256"]
            output = json.loads(path.read_text())
            original = json.loads((ROOT / f"{name}-documents.json").read_text())["documents"]
            validate_provenance(output["documents"], original)
            assert support.count(compact(output)) == output["tokens"] == row["output_tokens"] <= budget
            assert output["max_tokens"] == budget and row["status"] == output["status"]
            for doc in output["documents"]:
                ids = [block["id"] for block in doc["blocks"]]
                assert len(ids) == len(set(ids))
            kept = score_units(output["documents"], gold[name])
            assert kept == row["retained_units"]
            assert row["hits"] == len(kept) and row["total"] == len(gold[name])
            assert row["complete"] == (len(kept) == len(gold[name]))
            page = json.loads((ROOT / f"{name}-page.json").read_text())
            assert row["input_tokens"] == support.count(compact(page))
        for aggregate in results["aggregates"]:
            subset = [r for r in results["rows"] if r["budget"] == aggregate["budget"] and r["arm"] == aggregate["arm"]]
            assert aggregate["cases"] == len(subset) == 8
            for key in ("hits", "total", "input_tokens", "output_tokens"):
                assert aggregate[key] == sum(row[key] for row in subset)
            assert aggregate["complete_cases"] == sum(row["complete"] for row in subset)
    finally:
        support.close()
    comparisons = []
    for budget in frozen["budgets"]:
        counts = {"improved": 0, "unchanged": 0, "regressed": 0, "previously_kept_units_lost": 0}
        for question in questions:
            jp, en = (rows[(question["id"], budget, arm)] for arm in frozen["arms"])
            counts["improved" if en["hits"] > jp["hits"] else "regressed" if en["hits"] < jp["hits"] else "unchanged"] += 1
            counts["previously_kept_units_lost"] += len(set(jp["retained_units"]) - set(en["retained_units"]))
        comparisons.append({"budget": budget, **counts})
    result = {"status": "pass", "saved_outputs_checked": len(rows),
              "frozen_files_checked": len(frozen["files"]),
              "provenance_and_dependencies": "pass", "exact_token_counts_and_budgets": "pass",
              "recomputed_scores_and_aggregates": "pass", "comparisons": comparisons,
              "selector_rerun": False, "network_used": False,
              "results_sha256": digest(ROOT / "results.json")}
    write_json(ROOT / "verification.json", result)
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
