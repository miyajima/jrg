#!/usr/bin/env python3
"""Frozen three-arm comparison: old English/off, same English/on, new English/on."""
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
spec = importlib.util.spec_from_file_location("original_support", HELPER)
helper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helper)
Support, compact = helper.Support, helper.compact
write_json, validate_provenance = helper.write_json, helper.validate_provenance
ARMS = {"A": ("baseline_english.json", "off"), "B": ("baseline_english.json", "builtin"),
        "C": ("improved_english.json", "builtin")}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def freeze():
    if (ROOT / "frozen_inputs.json").exists():
        raise RuntimeError("Never overwrite frozen inputs")
    questions = json.loads((ROOT / "questions.json").read_text())["questions"]
    generation = json.loads((ROOT / "generation_manifest.json").read_text())
    for name, expected in generation["sha256"].items():
        assert digest(ROOT / name) == expected
    gold = {row["id"]: row["units"] for row in json.loads((ROOT / "gold.json").read_text())["cases"]}
    fetches = {row["id"]: row for row in json.loads((ROOT / "fetch_manifest.json").read_text())}
    paths = [ROOT / name for name in ("questions.json", "translator_questions.json", "generation_manifest.json",
        "baseline_instruction.txt", "improved_instruction.txt", "baseline_english.json", "improved_english.json",
        "fetch_manifest.json", "fetch_manifest_attempt-1.json", "prepared_manifest.json", "gold.json", "gold_review.json",
        "PLAN.md", "prepare.py", "html_extract.py", "score.py", "evaluate.py", "novelty_check.json")]
    paths += sorted((REPO / "src").rglob("*.rs"))
    paths += [HELPER, REPO / "data/query-expansions.v1.json", REPO / "Cargo.toml", REPO / "Cargo.lock",
              REPO / "integrations/mcp/jrg_select_proxy.py", REPO / "target/debug/jrg-select",
              REPO / "target/debug/examples/web_selection_eval_support", ROOT / "baseline-jrg-select"]
    for question in questions:
        name = question["id"]
        assert generation["frozen_before_fetch_at"] < fetches[name]["retrieved_at"]
        assert digest(ROOT / "raw" / f"{name}.html") == fetches[name]["sha256"]
        document = json.loads((ROOT / f"{name}-documents.json").read_text())["documents"][0]
        ids = {block["id"] for block in document["blocks"]}
        assert gold[name]
        for unit in gold[name]:
            assert unit["url"] == document["url"]
            assert unit["alternatives"] and all(group and set(group) <= ids for group in unit["alternatives"])
        paths += [ROOT / "raw" / f"{name}.html", ROOT / f"{name}.md",
                  ROOT / f"{name}-page.json", ROOT / f"{name}-documents.json"]
    for english_file in ("baseline_english.json", "improved_english.json"):
        assert [q["id"] for q in questions] == [q["id"] for q in json.loads((ROOT / english_file).read_text())]
    write_json(ROOT / "frozen_inputs.json", {
        "frozen_at": datetime.now(timezone.utc).isoformat(), "primary_budget": 1024, "budgets": [1024, 2048],
        "arms": {arm: {"english_file": file, "query_rules": mode} for arm, (file, mode) in ARMS.items()},
        "runs_per_condition": 1, "files": {str(p.relative_to(REPO)): digest(p) for p in paths},
    })
    print("Inputs and policy frozen before any selection")


def verify_frozen(manifest):
    for path, expected in manifest["files"].items():
        assert digest(REPO / path) == expected, f"Frozen file changed: {path}"


def run():
    manifest = json.loads((ROOT / "frozen_inputs.json").read_text())
    verify_frozen(manifest)
    if (ROOT / "results.json").exists() or (ROOT / "outputs").exists():
        raise RuntimeError("Preserve the single run")
    (ROOT / "outputs").mkdir()
    started = datetime.now(timezone.utc).isoformat()
    questions = json.loads((ROOT / "questions.json").read_text())["questions"]
    english = {name: {q["id"]: q["query_en"] for q in json.loads((ROOT / name).read_text())} for name, _ in ARMS.values()}
    gold = {case["id"]: case["units"] for case in json.loads((ROOT / "gold.json").read_text())["cases"]}
    rows, support = [], Support()
    try:
        for question in questions:
            name = question["id"]
            page = json.loads((ROOT / f"{name}-page.json").read_text())
            original = json.loads((ROOT / f"{name}-documents.json").read_text())["documents"]
            for budget in manifest["budgets"]:
                for arm, (file, mode) in ARMS.items():
                    request = {"query": question["question_ja"], "query_expansions": [english[file][name]],
                               "max_tokens": budget, "input": page}
                    process = subprocess.run([str(REPO / "target/debug/jrg-select"), "--query-rules", mode,
                        "--adapter", "markdown-page", "-"], input=compact(request), capture_output=True,
                        text=True, check=True, timeout=30)
                    value = json.loads(process.stdout)
                    validate_provenance(value["documents"], original)
                    assert support.count(compact(value)) == value["tokens"] <= budget
                    kept = score_units(value["documents"], gold[name])
                    output = f"outputs/{name}-{budget}-{arm}.json"
                    write_json(ROOT / output, value)
                    rows.append({"id": name, "budget": budget, "arm": arm, "retained_units": kept,
                        "hits": len(kept), "total": len(gold[name]), "complete": len(kept) == len(gold[name]),
                        "output_tokens": value["tokens"], "input_tokens": support.count(compact(page)),
                        "status": value["status"], "provenance_pass": True, "output_file": output,
                        "sha256": digest(ROOT / output)})
                    print(f"{name} {budget} {arm}: {len(kept)}/{len(gold[name])}", flush=True)
    finally:
        support.close()
    verify_frozen(manifest)
    aggregates = []
    for budget in manifest["budgets"]:
        for arm in ARMS:
            group = [row for row in rows if row["budget"] == budget and row["arm"] == arm]
            aggregates.append({"budget": budget, "arm": arm, "cases": len(group),
                **{key: sum(row[key] for row in group) for key in ("hits", "total", "input_tokens", "output_tokens")},
                "complete_cases": sum(row["complete"] for row in group)})
    write_json(ROOT / "results.json", {"started_at": started, "finished_at": datetime.now(timezone.utc).isoformat(),
        "frozen_inputs_sha256": digest(ROOT / "frozen_inputs.json"), "rows": rows, "aggregates": aggregates,
        "measurement": "Predeclared original-block units, full compact result JSON tokens using o200k_base; not answer accuracy or billing."})
    print(json.dumps(aggregates, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--freeze-only", action="store_true")
    args = parser.parse_args()
    freeze() if args.freeze_only else run()
