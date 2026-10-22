#!/usr/bin/env python3
"""Replay real captured payloads over local stdio MCP; never accesses the network."""
import json
from pathlib import Path
import sys

root = Path(__file__).resolve().parent
for line in sys.stdin:
    request = json.loads(line)
    if "id" not in request:
        continue
    method = request.get("method")
    if method == "initialize":
        result = {"protocolVersion": "2025-03-26", "capabilities": {"tools": {}},
                  "serverInfo": {"name": "real-web-corpus-replay", "version": "1"}}
    elif method == "tools/list":
        result = {"tools": [{"name": name, "description": "Captured public web payload",
                             "inputSchema": {"type": "object", "properties": {}}}
                            for name in ("search", "fetch")]}
    elif method == "tools/call":
        filename = "search-python.json" if request["params"]["name"] == "search" else "page-node.json"
        data = json.loads((root/filename).read_text())
        result = {"content": [{"type": "text", "text": json.dumps(data, ensure_ascii=False)}],
                  "structuredContent": data}
    else:
        raise ValueError("unsupported replay method")
    print(json.dumps({"jsonrpc": "2.0", "id": request["id"], "result": result}, ensure_ascii=False), flush=True)
