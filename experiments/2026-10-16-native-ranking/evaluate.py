"""Development-only CLI replay; no models, new downloads, or held-out queries."""
import argparse
import hashlib
import json
from pathlib import Path
import random
import re
import subprocess
import tempfile
import time

import tiktoken

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
OLD = ROOT / "experiments/2026-10-16-bm25-evaluation"
SOURCES = OLD / "private/sources"
SENTINEL = "zyxwvutsrqponmlkjihgfedcbazyxwvutsrqponmlkjihgfedcba"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify_sources(manifest):
    for entry in manifest:
        assert digest(SOURCES / entry["project"] / entry["path"]) == entry["sha256"], entry


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--run", action="store_true")
    options = parser.parse_args()
    protocol_file = HERE / "protocol.json"
    binary = ROOT / "target/release/jrg"
    manifest = json.loads((OLD / "source_manifest.json").read_text())
    verify_sources(manifest)
    if not options.run:
        if protocol_file.exists():
            raise SystemExit("protocol already exists; preserve it and use a new cohort")
        questions = json.loads((OLD / "questions.json").read_text())
        replacements = json.loads((OLD / "question-errata.json").read_text())["replacements"]
        questions = [q for q in questions if q["split"] == "dev" and q["mode"] == "rerank"]
        assert len(questions) == 10
        for q in questions:
            q["facts"] = replacements.get(q["id"], q["facts"])
        code = {str(p.relative_to(ROOT)): digest(p) for p in sorted((ROOT / "src").glob("*.rs"))}
        code.update({name: digest(ROOT / name) for name in ["Cargo.toml", "Cargo.lock"]})
        schedule = [(q["id"], arm) for q in questions for arm in ["existing", "path_control", "bm25"]]
        random.Random(20261016).shuffle(schedule)
        protocol = dict(questions=questions, schedule=schedule, code=code,
                        source_manifest_sha256=digest(OLD / "source_manifest.json"),
                        binary_sha256=digest(binary), evaluator_sha256=digest(Path(__file__)),
                        query_sha256=digest(OLD / "questions.json"),
                        errata_sha256=digest(OLD / "question-errata.json"),
                        max_tokens=2000, context=8, threads=1, seed=20261016,
                        purpose="known development questions; diagnostic, not independent validation",
                        primary_metric="full registered evidence lines delivered within 2000 tokens",
                        control="same binary and capture pipeline; path_control uses OOV rank query",
                        candidate_contract="same explicit frozen file list and regex; no new candidate recovery")
        protocol_file.write_text(json.dumps(protocol, ensure_ascii=False, indent=2) + "\n")
        print("Registered 10 development queries x 3 arms before retrieval.")
        return
    protocol = json.loads(protocol_file.read_text())
    assert digest(binary) == protocol["binary_sha256"]
    assert digest(Path(__file__)) == protocol["evaluator_sha256"]
    assert digest(OLD / "source_manifest.json") == protocol["source_manifest_sha256"]
    assert all(digest(ROOT / path) == value for path, value in protocol["code"].items())
    raw = HERE / "raw"
    raw.mkdir(exist_ok=True)
    rows = []
    tokenizer = tiktoken.get_encoding("o200k_base")
    by_id = {q["id"]: q for q in protocol["questions"]}
    for query_id, arm in protocol["schedule"]:
        q = by_id[query_id]
        run_id = f"{query_id}-{arm}"
        if (raw / f"{run_id}.json").exists():
            raise SystemExit("run exists; no automatic retries")
        paths = [e["path"] for e in manifest if e["project"] == q["project"]]
        assert not any(SENTINEL in (SOURCES / q["project"] / p).read_text().lower() for p in paths)
        with tempfile.TemporaryDirectory(prefix="jrg-rank-replay-") as state:
            argv = [str(binary), "--smart", "--regex", "--engine", "native", "-j1", "-C8",
                    "--max-tokens", "2000", "--session", str(Path(state) / "state.json")]
            if arm != "existing":
                argv += ["--rank-query", q["query"] if arm == "bm25" else SENTINEL]
            argv += [q["candidate_pattern"], *paths]
            (raw / f"{run_id}.intent.json").write_text(json.dumps({"argv": argv}))
            start = time.perf_counter()
            out = subprocess.run(argv, cwd=SOURCES / q["project"], stdin=subprocess.DEVNULL,
                                 capture_output=True, text=True, timeout=60)
            elapsed = time.perf_counter() - start
        stdout = out.stdout
        (raw / f"{run_id}.txt").write_text(stdout)
        delivered = {}
        current = None
        for line in stdout.splitlines():
            if line.startswith("@ "):
                current = line[2:].rsplit(":", 1)[0].removeprefix("./")
            elif current and re.match(r"^\d+:", line):
                delivered.setdefault(current, set()).add(int(line.split(":", 1)[0]))
        covered = all(set(range(f["start_line"], f["end_line"] + 1)) <= delivered.get(f["path"], set()) for f in q["facts"])
        row = dict(id=run_id, query_id=query_id, arm=arm, language=q["language"],
                   exit_code=out.returncode, stderr=out.stderr, seconds=elapsed,
                   tokens=len(tokenizer.encode_ordinary(stdout)), evidence_complete=covered,
                   capture_omitted=int(re.search(r"capture_omitted=(\d+)", stdout)[1]),
                   untracked=int(re.search(r"untracked=(\d+)", stdout)[1]),
                   pending=int(re.search(r"pending=(\d+)", stdout)[1]))
        (raw / f"{run_id}.json").write_text(json.dumps(row, ensure_ascii=False, indent=2) + "\n")
        rows.append(row)
        print(f"{run_id}: evidence={covered} tokens={row['tokens']} capture_omitted={row['capture_omitted']}", flush=True)
    verify_sources(manifest)
    assert digest(binary) == protocol["binary_sha256"]
    assert all(digest(ROOT / path) == value for path, value in protocol["code"].items())
    assert len({r["id"] for r in rows}) == 30
    assert all(r["tokens"] <= protocol["max_tokens"] for r in rows)
    summary = {arm: dict(complete=sum(r["evidence_complete"] for r in rows if r["arm"] == arm),
                         total=sum(r["arm"] == arm for r in rows),
                         failures=sum(r["exit_code"] != 0 for r in rows if r["arm"] == arm),
                         capture_limited=sum(r["capture_omitted"] > 0 for r in rows if r["arm"] == arm))
               for arm in ["existing", "path_control", "bm25"]}
    (HERE / "results.json").write_text(json.dumps(dict(summary=summary, rows=rows,
        source_and_code_integrity=True, protocol_sha256=digest(protocol_file)), ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(summary, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
