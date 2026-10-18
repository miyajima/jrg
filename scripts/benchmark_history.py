#!/usr/bin/env python3
"""Measure history overhead in an isolated Git fixture; never uses user history.

Usage: python3 scripts/benchmark_history.py --binary target/release/jrg
Results are synthetic latency measurements, not search-quality measurements.
"""
import argparse
import hashlib
import json
import math
import os
import platform
from pathlib import Path
import random
import sqlite3
import statistics
import subprocess
import tempfile
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/jrg"))
    parser.add_argument("--repeats", type=int, default=25)
    options = parser.parse_args()
    binary = options.binary.resolve(strict=True)
    if not 5 <= options.repeats <= 100:
        parser.error("repeats must be 5..100")
    with tempfile.TemporaryDirectory(prefix="jrg-history-bench-") as temporary:
        base = Path(temporary)
        root = base / "repo"
        state = base / "state"
        root.mkdir()
        for index in range(40):
            text = "\n".join(
                f"fn cancel_audio_{index}_{n}(active: bool) {{\n"
                f"    if active {{ stop_audio_{index}_{n}(); }}\n}}"
                for n in range(15)
            )
            (root / f"audio_{index:02}.rs").write_text(text + "\n")
        git_env = dict(os.environ, GIT_AUTHOR_NAME="Fixture", GIT_COMMITTER_NAME="Fixture",
                       GIT_AUTHOR_EMAIL="fixture@example.invalid", GIT_COMMITTER_EMAIL="fixture@example.invalid")

        def git(*args):
            subprocess.run(["git", "-c", "core.hooksPath=/dev/null", "-c", "commit.gpgsign=false", *args],
                           cwd=root, env=git_env, check=True, capture_output=True)

        git("init", "-q")
        git("add", ".")
        git("commit", "-qm", "fixture")
        env = dict(os.environ, JRG_STATE_DIR=str(state))
        env.pop("JRG_HISTORY", None)
        env.pop("JRG_EMBEDDING_SOCKET", None)
        args = ["--smart", "--intent", "review", "--rank-query", "cancel audio", "cancel_audio", "."]

        def run(argv, cwd=root):
            started = time.perf_counter_ns()
            result = subprocess.run([str(binary), *argv], cwd=cwd, env=env, capture_output=True, check=True)
            return (time.perf_counter_ns() - started) / 1e6, result.stdout

        def latest(cwd=root):
            listing = json.loads(run(["--history", "list"], cwd)[1])
            request_id = listing["requests"][0]["request_id"]
            return json.loads(run(["--history", "show", request_id], cwd)[1])["request"]

        def measure_mcp():
            child = subprocess.Popen([str(binary), "--smart", "--mcp", "--root", str(root)],
                                     cwd=root, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                     stderr=subprocess.PIPE, text=True)
            serial = 0

            def call(method, params):
                nonlocal serial
                serial += 1
                request = {"jsonrpc": "2.0", "id": serial, "method": method, "params": params}
                started = time.perf_counter_ns()
                child.stdin.write(json.dumps(request) + "\n")
                child.stdin.flush()
                response = json.loads(child.stdout.readline())
                duration = (time.perf_counter_ns() - started) / 1e6
                if response.get("error") or response.get("result", {}).get("isError"):
                    raise RuntimeError(response)
                return duration, response["result"]

            call("initialize", {})
            child.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n")
            query = {"operations": [{"search": {"patterns": ["cancel_audio"], "paths": ["."], "intent": "review"}}],
                     "rank_query": "cancel audio", "context_id": "benchmark-task"}
            _, initial = call("tools/call", {"name": "retrieve", "arguments": query})
            history = initial["structuredContent"]["history"]
            call("tools/call", {"name": "feedback", "arguments": {
                "request_id": history["request_id"], "result_id": history["results"][0]["result_id"],
                "label": "helpful", "event_id": f"benchmark-{time.perf_counter_ns()}", "context_id": "benchmark-task"}})
            values = {mode: [] for mode in ("off", "record", "auto")}
            for _ in range(options.repeats):
                modes = list(values)
                randomizer.shuffle(modes)
                for mode in modes:
                    values[mode].append(call("tools/call", {"name": "retrieve", "arguments": dict(query, history_mode=mode)})[0])
            child.stdin.close()
            if child.wait() != 0:
                raise RuntimeError(child.stderr.read())
            return {mode: {"median_ms": round(statistics.median(v), 3),
                           "p95_ms": round(sorted(v)[math.ceil(len(v) * .95) - 1], 3)}
                    for mode, v in values.items()}

        cold_off, _ = run([*args, "--history-mode", "off"])
        cold_auto, _ = run(args)
        original = latest()
        run(["--history", "feedback", original["request_id"], original["results"][0]["result_id"], "helpful"])
        samples = {mode: [] for mode in ("off", "record", "auto")}
        randomizer = random.Random(17)
        for _ in range(options.repeats):
            modes = list(samples)
            randomizer.shuffle(modes)
            for mode in modes:
                duration, _ = run([*args, "--history-mode", mode])
                samples[mode].append(duration)
        summary = {mode: {"median_ms": round(statistics.median(values), 3),
                          "p95_ms": round(sorted(values)[math.ceil(len(values) * .95) - 1], 3)}
                   for mode, values in samples.items()}
        off = summary["off"]["p95_ms"]
        overhead = summary["auto"]["p95_ms"] - off
        worktree = base / "worktree"
        git("worktree", "add", "--detach", str(worktree))
        first_worktree, _ = run(args, worktree)
        worktree_record = latest(worktree)
        path = worktree / worktree_record["results"][0]["path"]
        path.write_text("// unrelated edit\n" + path.read_text())
        after_edit, _ = run(args, worktree)
        edit_record = latest(worktree)
        mcp_warm = measure_mcp()
        # Populate only this fixture's DB to measure history growth near retention.
        with sqlite3.connect(state / "history-v1.sqlite3") as database:
            count = database.execute("SELECT COUNT(*) FROM requests").fetchone()[0]
            for index in range(max(0, 9999 - count)):
                request = dict(original, request_id=f"benchmark-filled-{index}", results=[],
                               created_at=original["created_at"] - 60_000 + index)
                database.execute("INSERT INTO requests VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                                 (request["request_id"], request["repo_id"], request["worktree_id"],
                                  "emitted", request["created_at"], request["query_key"],
                                  request["context_id"], json.dumps(request)))
        filled_samples = {mode: [] for mode in ("off", "auto")}
        for _ in range(options.repeats):
            modes = list(filled_samples)
            randomizer.shuffle(modes)
            for mode in modes:
                filled_samples[mode].append(run([*args, "--history-mode", mode])[0])
        filled = {mode: {"median_ms": round(statistics.median(values), 3),
                         "p95_ms": round(sorted(values)[math.ceil(len(values) * .95) - 1], 3)}
                  for mode, values in filled_samples.items()}
        mcp_filled = measure_mcp()
        report = {
            "binary_version": run(["--version"])[1].decode().strip(),
            "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            "platform": platform.platform(),
            "fixture": {"files": 40, "functions": 600, "embedding": "unconfigured"},
            "repeats": options.repeats,
            "cold_process_off_ms": round(cold_off, 3),
            "first_history_process_ms": round(cold_auto, 3),
            "warm": summary,
            "auto_p95_overhead_ms": round(overhead, 3),
            "target_max_ms": round(max(10, off * .1), 3),
            "target_met": overhead <= max(10, off * .1),
            "first_worktree_process_ms": round(first_worktree, 3),
            "worktree_feedback_applied": worktree_record["history_applied"],
            "after_external_edit_process_ms": round(after_edit, 3),
            "edited_feedback_applied": edit_record["history_applied"],
            "near_10000_requests": filled,
            "persistent_mcp_warm": mcp_warm,
            "persistent_mcp_near_10000_requests": mcp_filled,
            "notes": "Includes process startup and fresh source search. Synthetic speed only; no quality or token-savings conclusion."
        }
        for name, result in (("cli_near_10000", filled), ("mcp_warm", mcp_warm), ("mcp_near_10000", mcp_filled)):
            baseline = result["off"]["p95_ms"]
            extra = result["auto"]["p95_ms"] - baseline
            report[name + "_overhead"] = {"p95_ms": round(extra, 3), "target_max_ms": round(max(10, .1 * baseline), 3),
                                               "target_met": extra <= max(10, .1 * baseline)}
        print(json.dumps(report, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
