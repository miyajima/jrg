#!/usr/bin/env python3
"""Check persisted results without running a model, network fetch or selector."""
import json
from evaluate import ROOT, ARMS, Support, compact, digest, verify_frozen, validate_provenance, write_json
from score import score_units


def main():
    frozen = json.loads((ROOT / "frozen_inputs.json").read_text())
    verify_frozen(frozen)
    result = json.loads((ROOT / "results.json").read_text())
    assert digest(ROOT / "frozen_inputs.json") == result["frozen_inputs_sha256"]
    assert frozen["frozen_at"] < result["started_at"] < result["finished_at"]
    questions = json.loads((ROOT / "questions.json").read_text())["questions"]
    gold = {c["id"]: c["units"] for c in json.loads((ROOT / "gold.json").read_text())["cases"]}
    rows = {(r["id"], r["budget"], r["arm"]): r for r in result["rows"]}
    expected = {(q["id"], budget, arm) for q in questions for budget in frozen["budgets"] for arm in ARMS}
    assert set(rows) == expected and len(rows) == len(result["rows"]) == 48
    assert len(list((ROOT / "outputs").glob("*.json"))) == 48
    support = Support()
    try:
        for (name, budget, arm), row in rows.items():
            path = ROOT / row["output_file"]
            assert digest(path) == row["sha256"]
            value = json.loads(path.read_text())
            original = json.loads((ROOT / f"{name}-documents.json").read_text())["documents"]
            validate_provenance(value["documents"], original)
            assert support.count(compact(value)) == value["tokens"] == row["output_tokens"] <= budget
            assert value["max_tokens"] == budget and value["status"] == row["status"]
            for document in value["documents"]:
                ids = [b["id"] for b in document["blocks"]]
                assert len(ids) == len(set(ids))
            kept = score_units(value["documents"], gold[name])
            assert kept == row["retained_units"] and row["hits"] == len(kept)
            assert row["total"] == len(gold[name]) and row["complete"] == (len(kept) == len(gold[name]))
            page = json.loads((ROOT / f"{name}-page.json").read_text())
            assert support.count(compact(page)) == row["input_tokens"]
        for aggregate in result["aggregates"]:
            group = [r for r in result["rows"] if r["budget"] == aggregate["budget"] and r["arm"] == aggregate["arm"]]
            assert len(group) == aggregate["cases"] == 8
            for field in ("hits", "total", "input_tokens", "output_tokens"):
                assert aggregate[field] == sum(row[field] for row in group)
            assert aggregate["complete_cases"] == sum(row["complete"] for row in group)
    finally:
        support.close()
    comparisons = []
    for budget in frozen["budgets"]:
        for base, compared in (("A", "B"), ("A", "C"), ("B", "C")):
            counts = {"improved": 0, "unchanged": 0, "regressed": 0, "previously_kept_units_lost": 0}
            for question in questions:
                a, b = rows[(question["id"], budget, base)], rows[(question["id"], budget, compared)]
                key = "improved" if b["hits"] > a["hits"] else "regressed" if b["hits"] < a["hits"] else "unchanged"
                counts[key] += 1
                counts["previously_kept_units_lost"] += len(set(a["retained_units"]) - set(b["retained_units"]))
            comparisons.append({"budget": budget, "base": base, "compared": compared, **counts})
    verification = {"status": "pass", "saved_outputs_checked": 48, "frozen_files_checked": len(frozen["files"]),
        "exact_token_counts_and_budget": "pass", "provenance_and_dependencies": "pass",
        "scores_and_aggregates": "pass", "selector_rerun": False, "network_used": False,
        "results_sha256": digest(ROOT / "results.json"), "comparisons": comparisons}
    write_json(ROOT / "verification.json", verification)
    print(json.dumps(verification, indent=2))


if __name__ == "__main__":
    main()
