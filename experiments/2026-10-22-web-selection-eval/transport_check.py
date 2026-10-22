#!/usr/bin/env python3
"""Real captured content -> actual MCP/RTK proxy -> local receiving endpoint."""
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import argparse
import json
from pathlib import Path
import signal
import subprocess
import sys
import threading
import urllib.request
from evaluate import ROOT, REPO, Support, compact, write_json


def main(bilingual=False):
    budget = 1024
    all_cases = json.loads((ROOT/"cases.json").read_text())["cases"]
    language = "japanese" if bilingual else "source_language"
    cases = [next(c for c in all_cases if c["id"] == key)
             for key in (f"search-python-{language}", f"page-node-{language}")]

    def english(case):
        return next(c["query"] for c in all_cases if c["id"] == f"{case['stage']}-{case['topic']}-source_language")

    def selector_expansions(case):
        return {"query_expansions":[english(case)]} if bilingual else {}
    selector = str(REPO/"target/debug/jrg-select")
    proxy = subprocess.Popen([sys.executable, str(REPO/"integrations/mcp/jrg_select_proxy.py"),
                              "--selector", selector, "--max-tokens", str(budget),
                              "--upstream", sys.executable, str(ROOT/"replay_mcp.py")],
                             stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
    rows, support = [], Support()
    try:
        for n, case in enumerate(cases):
            source = json.loads((ROOT/case["input_file"]).read_text())
            tool = "search" if case["stage"] == "search" else "fetch"
            arguments = {"query": case["query"], "jrg_focus": case["query"]}
            if bilingual:
                arguments["jrg_focus_en"] = english(case)
            message = {"jsonrpc": "2.0", "id": n+1, "method": "tools/call",
                       "params": {"name": tool, "arguments": arguments}}
            proxy.stdin.write(compact(message)+"\n")
            proxy.stdin.flush()
            response = json.loads(proxy.stdout.readline())
            selected = json.loads(response["result"]["content"][0]["text"])
            expected = json.loads(subprocess.check_output([selector, "--adapter", case["adapter"], "-"],
                                                         input=compact({"query":case["query"],"max_tokens":budget,"input":source,**selector_expansions(case)}).encode()))
            assert selected == expected
            assert set(response["result"]) == {"content"}
            rows.append({"transport":"MCP","case":case["id"],"matches_direct_cli":True,
                         "raw_payload_tokens":support.count(compact(source)),
                         "result_envelope_tokens":support.count(compact(response["result"]))})
    finally:
        proxy.stdin.close()
        proxy.stdout.close()
        proxy.wait(timeout=15)

    forwarded = []

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass

        def do_POST(self):
            forwarded.append(json.loads(self.rfile.read(int(self.headers["Content-Length"]))))
            body = compact({"id":"local-only","type":"message","role":"assistant","model":"fixture",
                            "content":[{"type":"text","text":"ok"}],"stop_reason":"end_turn",
                            "usage":{"input_tokens":0,"output_tokens":0}}).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    binary = REPO/"integrations/rtk-proxy/target/debug/jrg-rtk-proxy"
    rtk = subprocess.Popen([str(binary), "--selector", selector, "--bind", "127.0.0.1:0",
                            "--max-tokens", str(budget), "--anthropic-upstream",
                            f"http://127.0.0.1:{server.server_port}"], stderr=subprocess.PIPE, text=True)
    try:
        line = rtk.stderr.readline()
        if "listening on " not in line:
            raise RuntimeError("RTK proxy did not start: "+line)
        address = line.split("listening on ", 1)[1].split(";", 1)[0]
        for case in cases:
            source = json.loads((ROOT/case["input_file"]).read_text())
            tool = "search" if case["stage"] == "search" else "fetch"
            request = {"model":"fixture-only","max_tokens":10,"messages":[
                {"role":"user","content":case["query"]},
                {"role":"assistant","content":[{"type":"tool_use","id":case["id"],"name":tool,
                                                 "input":{"jrg_focus":case["query"],**({"jrg_focus_en":english(case)} if bilingual else {})}}]},
                {"role":"user","content":[{"type":"tool_result","tool_use_id":case["id"],
                                            "content":compact(source)}]}]}
            http = urllib.request.Request(f"http://{address}/v1/messages", data=compact(request).encode(),
                                          headers={"Content-Type":"application/json","x-api-key":"local-fixture-only",
                                                   "anthropic-version":"2023-06-01"})
            with urllib.request.urlopen(http, timeout=20) as response:
                assert response.status == 200
            block = forwarded[-1]["messages"][-1]["content"][0]
            selected = json.loads(block["content"])["selection"]
            expected = json.loads(subprocess.check_output([selector, "--adapter", case["adapter"], "-"],
                                                         input=compact({"query":case["query"],"max_tokens":budget,"input":source,**selector_expansions(case)}).encode()))
            assert not block["is_error"] and selected == expected
            rows.append({"transport":"RTK","case":case["id"],"matches_direct_cli":True,
                         "raw_payload_tokens":support.count(compact(source)),
                         "result_envelope_tokens":support.count(compact(block))})
    finally:
        rtk.send_signal(signal.SIGINT)
        rtk.wait(timeout=15)
        rtk.stderr.close()
        server.shutdown()
        server.server_close()
        support.close()
    write_json(ROOT/("bilingual_transport_results.json" if bilingual else "transport_results.json"), rows)
    print(json.dumps(rows, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--bilingual", action="store_true")
    main(parser.parse_args().bilingual)
