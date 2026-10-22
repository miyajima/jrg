#!/usr/bin/env python3
"""One offline paired run of the preregistered held-out page experiment."""
import argparse
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess

from score import score_units

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parents[1]
HELPER = ROOT.parent / "2026-10-22-web-selection-eval/evaluate.py"
spec = importlib.util.spec_from_file_location("previous_evaluation", HELPER)
previous = importlib.util.module_from_spec(spec)
spec.loader.exec_module(previous)
Support, compact = previous.Support, previous.compact
write_json, validate_provenance = previous.write_json, previous.validate_provenance


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def freeze():
    path = ROOT / "frozen_inputs.json"
    if path.exists():
        raise RuntimeError("Inputs already frozen; do not overwrite the preregistration")
    questions = json.loads((ROOT / "questions.json").read_text())["questions"]
    gold = {case["id"]: case["units"] for case in json.loads((ROOT / "gold.json").read_text())["cases"]}
    english = json.loads((ROOT / "generated_english.json").read_text())
    assert [q["id"] for q in questions] == [q["id"] for q in english]
    generation = json.loads((ROOT / "generation_manifest.json").read_text())
    assert digest(ROOT / "generated_english.json") == generation["output_sha256"]
    assert digest(ROOT / "translator_questions.json") == generation["questions_sha256"]
    paths = [ROOT / name for name in (
        "questions.json", "translator_questions.json", "translation_instruction.txt",
        "generated_english.json", "generation_manifest.json", "fetch_manifest.json",
        "prepared_manifest.json", "gold.json", "PLAN.md", "html_extract.py",
        "prepare.py", "score.py", "test_score.py", "test_html_extract.py", "evaluate.py",
        "novelty_check.json", "translation_audit.json", "engine_unchanged.json", "gold_review.json",
    )]
    paths += [HELPER, REPO / "Cargo.lock", REPO / "Cargo.toml",
              REPO / "target/debug/jrg-select", REPO / "target/debug/examples/web_selection_eval_support"]
    paths += sorted((REPO / "src").rglob("*.rs"))
    fetches = {row["id"]: row for row in json.loads((ROOT / "fetch_manifest.json").read_text())}
    for question in questions:
        name = question["id"]
        paths += [ROOT / "raw" / f"{name}.html", ROOT / f"{name}.md",
                  ROOT / f"{name}-page.json", ROOT / f"{name}-documents.json"]
        assert digest(ROOT / "raw" / f"{name}.html") == fetches[name]["sha256"]
        assert generation["frozen_before_source_fetch_at"] < fetches[name]["retrieved_at"]
        document = json.loads((ROOT / f"{name}-documents.json").read_text())["documents"][0]
        ids = {block["id"] for block in document["blocks"]}
        for unit in gold[name]:
            assert unit["url"] == document["url"]
            assert unit["alternatives"] and all(group and set(group) <= ids for group in unit["alternatives"])
    write_json(path, {
        "frozen_at": datetime.now(timezone.utc).isoformat(),
        "primary_budget": 1024, "budgets": [512, 1024, 2048],
        "arms": ["japanese", "automatic_english"], "runs_per_condition": 1,
        "files": {str(p.relative_to(REPO)): digest(p) for p in paths},
    })
    print("Inputs frozen before selection")


def verify_frozen(manifest):
    for path, expected in manifest["files"].items():
        if digest(REPO / path) != expected:
            raise AssertionError(f"Frozen input changed: {path}")


def run():
    frozen = json.loads((ROOT / "frozen_inputs.json").read_text())
    verify_frozen(frozen)
    if (ROOT / "results.json").exists() or (ROOT / "outputs").exists():
        raise RuntimeError("Run artifacts already exist; preserve the single run")
    questions = json.loads((ROOT / "questions.json").read_text())["questions"]
    english = {row["id"]: row["query_en"] for row in json.loads((ROOT / "generated_english.json").read_text())}
    gold = {case["id"]: case["units"] for case in json.loads((ROOT / "gold.json").read_text())["cases"]}
    (ROOT / "outputs").mkdir()
    started = datetime.now(timezone.utc).isoformat()
    support, rows = Support(), []
    try:
        for question in questions:
            name = question["id"]
            page = json.loads((ROOT / f"{name}-page.json").read_text())
            documents = json.loads((ROOT / f"{name}-documents.json").read_text())["documents"]
            input_tokens = support.count(compact(page))
            for budget in frozen["budgets"]:
                for arm in frozen["arms"]:
                    request = {"query": question["question_ja"], "max_tokens": budget, "input": page}
                    if arm == "automatic_english":
                        request["query_expansions"] = [english[name]]
                    process = subprocess.run(
                        [str(REPO / "target/debug/jrg-select"), "--adapter", "markdown-page", "-"],
                        input=compact(request), text=True, capture_output=True, timeout=30, check=True,
                    )
                    result = json.loads(process.stdout)
                    validate_provenance(result["documents"], documents)
                    assert support.count(compact(result)) == result["tokens"] <= budget
                    kept = score_units(result["documents"], gold[name])
                    output_file = f"outputs/{name}-{budget}-{arm}.json"
                    write_json(ROOT / output_file, result)
                    rows.append({
                        "id": name, "budget": budget, "arm": arm,
                        "retained_units": kept, "hits": len(kept), "total": len(gold[name]),
                        "complete": len(kept) == len(gold[name]),
                        "input_tokens": input_tokens, "output_tokens": result["tokens"],
                        "status": result["status"], "provenance_pass": True,
                        "output_file": output_file, "sha256": digest(ROOT / output_file),
                    })
                    print(f"{name} {budget} {arm}: {len(kept)}/{len(gold[name])}, {result['tokens']} tokens", flush=True)
    finally:
        support.close()
    verify_frozen(frozen)
    aggregates = []
    for budget in frozen["budgets"]:
        for arm in frozen["arms"]:
            subset = [row for row in rows if row["budget"] == budget and row["arm"] == arm]
            aggregates.append({
                "budget": budget, "arm": arm, "cases": len(subset),
                "hits": sum(row["hits"] for row in subset), "total": sum(row["total"] for row in subset),
                "complete_cases": sum(row["complete"] for row in subset),
                "input_tokens": sum(row["input_tokens"] for row in subset),
                "output_tokens": sum(row["output_tokens"] for row in subset),
            })
    write_json(ROOT / "results.json", {
        "started_at": started, "finished_at": datetime.now(timezone.utc).isoformat(),
        "frozen_inputs_sha256": digest(ROOT / "frozen_inputs.json"),
        "measurement": "Exact o200k_base tokens in compact page JSON and complete compact selector result JSON; no answer grading or billing measurement.",
        "rows": rows, "aggregates": aggregates,
    })
    print(json.dumps(aggregates, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--freeze-only", action="store_true")
    args = parser.parse_args()
    freeze() if args.freeze_only else run()
