#!/usr/bin/env python3
"""Small JSONL MCP peer used only by selection_mcp.py."""

from __future__ import annotations

import argparse
import json
import os
import sys
import time


def send(message: object) -> None:
    sys.stdout.buffer.write(json.dumps(message, ensure_ascii=False, separators=(",", ":")).encode() + b"\n")
    sys.stdout.buffer.flush()


def response(message_id: object, result: object) -> None:
    send({"jsonrpc": "2.0", "id": message_id, "result": result,
          "_meta": {"raw": "RAW_NOISE_ENVELOPE_SENTINEL"}})


def error(message_id: object, code: int, message: str) -> None:
    send({"jsonrpc": "2.0", "id": message_id, "error": {"code": code, "message": message}})


def tool_schema(name: str, required: list[str]) -> dict:
    return {
        "name": name,
        "description": f"mock {name}",
        "inputSchema": {
            "type": "object",
            "properties": {key: {"type": "string"} for key in required},
            "required": required,
            "additionalProperties": False,
        },
        "outputSchema": {"type": "object", "properties": {"upstream": {"type": "string"}}},
    }


def selected_search_result() -> dict:
    return {
        "results": [
            {
                "id": "terms",
                "url": "https://example.test/terms",
                "title": "解約条件",
                "text": "解約時の違約金は無料です。返金条件を確認してください。",
            },
            {
                "id": "weather",
                "url": "https://example.test/weather",
                "title": "天気",
                "text": "RAW_NOISE_SEARCH_SENTINEL 晴れのち雨。",
            },
        ]
    }


def selected_english_search_result() -> dict:
    return {
        "results": [
            {
                "id": "english-terms",
                "url": "https://example.test/en/terms",
                "title": "Cancellation policy",
                "text": (
                    "To request cancellation of a subscription, submit the form at least "
                    "30 days before renewal. Contract version: 3.2."
                ),
            }
        ]
    }


def selected_page_result() -> dict:
    return {
        "id": "terms",
        "url": "https://example.test/terms",
        "title": "契約規約",
        "markdown": (
            "# 契約規約\n\n"
            "## 解約と違約金\n\n"
            "解約時の違約金は1万円です。[^1]\n\n"
            "[^1]: 初月は請求しません。\n\n"
            "## 支払方法\n\n"
            "RAW_NOISE_PAGE_SENTINEL 支払方法は銀行振込です。\n"
        ),
    }


def selected_english_page_result() -> dict:
    return {
        "id": "english-terms",
        "url": "https://example.test/en/terms",
        "title": "Terms of Service",
        "markdown": (
            "# Terms of Service\n\n"
            "## Cancellation\n\n"
            "Cancellation fees are waived if you submit a request at least 30 days before renewal.\n"
        ),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--mode", default="normal")
    parser.add_argument("--pid-file")
    args = parser.parse_args()
    if args.pid_file:
        with open(args.pid_file, "w", encoding="ascii") as output:
            output.write(str(os.getpid()))

    seen_calls: list[str] = []
    for raw in sys.stdin.buffer:
        try:
            message = json.loads(raw)
        except (UnicodeDecodeError, json.JSONDecodeError):
            continue
        if not isinstance(message, dict):
            continue
        method = message.get("method")
        message_id = message.get("id")
        if method == "notifications/initialized":
            continue
        if method == "notifications/cancelled":
            if args.mode == "cancel-aware":
                error("cancel-me", -32800, "mock request cancelled")
            continue
        if method == "initialize":
            response(
                message_id,
                {
                    "protocolVersion": "2025-03-26",
                    "capabilities": {"tools": {"listChanged": False}},
                    "serverInfo": {"name": "selection-mock", "version": "1"},
                },
            )
            continue
        if method == "tools/list":
            response(
                message_id,
                {
                    "tools": [
                        tool_schema("search", ["query"]),
                        tool_schema("fetch", ["url"]),
                        tool_schema("other", []),
                    ]
                },
            )
            continue
        if method != "tools/call":
            if message_id is not None:
                error(message_id, -32601, "method not found")
            continue

        params = message.get("params") or {}
        name = params.get("name")
        tool_args = params.get("arguments") or {}
        seen_calls.append(name)
        if name == "probe":
            response(message_id, {"content": [{"type": "text", "text": json.dumps({"calls": seen_calls})}]})
            continue
        if args.mode == "callback":
            send({"jsonrpc": "2.0", "id": "peer-callback", "method": "sampling/createMessage", "params": {}})
            time.sleep(10)
            continue
        if args.mode == "timeout":
            time.sleep(10)
            response(message_id, {"content": [{"type": "text", "text": "RAW_TIMEOUT_SENTINEL"}]})
            continue
        if args.mode == "oversized":
            raw_result = "RAW_OVERSIZED_SENTINEL" + ("x" * (4 * 1024 * 1024))
            send({"jsonrpc": "2.0", "id": message_id, "result": {"content": [{"type": "text", "text": raw_result}]}})
            continue
        if args.mode == "cancel-aware":
            continue

        if name == "rpc_error":
            send({"jsonrpc": "2.0", "id": message_id, "error": {"code": 1234, "message": "mock upstream failure", "data": {"detail": "preserve"}}})
        elif name == "other":
            response(message_id, {"content": [{"type": "text", "text": "unchanged"}], "structuredContent": {"passthrough": True, "arguments": tool_args}, "_meta": {"opaque": "unchanged"}})
        elif name == "search" and tool_args.get("query") == "tool-error":
            response(message_id, {"content": [{"type": "text", "text": "mock tool failure"}], "isError": True, "structuredContent": {"secret": "TOOL_ERROR_SENTINEL"}, "_meta": {"secret": "TOOL_ERROR_META_SENTINEL"}})
        elif name == "search" and tool_args.get("query") == "bad-envelope":
            send({"jsonrpc":"2.0","id":message_id,"error":{"code":-32000,"message":"failure"},"result":{"structuredContent":{"raw":"RAW_BAD_ENVELOPE"}},"_meta":{"raw":"RAW_BAD_ENVELOPE"}})
        elif name == "search" and tool_args.get("query") == "bad-error":
            send({"jsonrpc":"2.0","id":message_id,"error":"RAW_BAD_ENVELOPE"})
        elif name == "search" and tool_args.get("query") == "bad-method-envelope":
            send({"jsonrpc":"2.0","id":message_id,"method":42,"error":{"code":-32000,"message":"failure"},"result":{"raw":"RAW_BAD_ENVELOPE"}})
        elif name == "search" and tool_args.get("query") == "bad-structured":
            response(message_id, {"structuredContent": "RAW_NOISE_MALFORMED_SENTINEL", "content": [{"type": "text", "text": json.dumps(selected_search_result())}]})
        elif name == "search" and tool_args.get("query") == "bad-text":
            response(message_id, {"content": [{"type": "text", "text": "not JSON RAW_NOISE_MALFORMED_SENTINEL"}]})
        elif name == "search" and tool_args.get("query") == "mixed-media":
            response(message_id, {"content": [{"type": "text", "text": json.dumps(selected_search_result())}, {"type": "image", "data": "RAW_NOISE_MALFORMED_SENTINEL"}]})
        elif name == "search" and tool_args.get("query") == "structured-mixed-media":
            response(message_id, {"structuredContent": selected_search_result(), "content": [{"type": "text", "text": "RAW_NOISE_STRUCTURED_MIXED_SENTINEL"}, {"type": "image", "data": "RAW_NOISE_STRUCTURED_MIXED_SENTINEL"}]})
        elif name == "search" and tool_args.get("query") == "bad-input":
            response(message_id, {"structuredContent": {"unexpected": "RAW_NOISE_SELECTION_SENTINEL"}, "_meta": {"raw": "RAW_NOISE_SELECTION_SENTINEL"}})
        elif name == "search":
            if "jrg_focus" in tool_args or "jrg_focus_en" in tool_args:
                response(message_id, {"content": [{"type": "text", "text": "selection-only focus arguments must be stripped"}], "isError": True})
            else:
                search_result = (
                    selected_english_search_result()
                    if tool_args.get("query") == "bilingual"
                    else selected_search_result()
                )
                response(
                    message_id,
                    {
                        "content": [{"type": "text", "text": "RAW_NOISE_CONTENT_SENTINEL"}],
                        "structuredContent": search_result,
                        "_meta": {"trace": "RAW_NOISE_META_SENTINEL"},
                    },
                )
        elif name == "fetch":
            if "jrg_focus" in tool_args or "jrg_focus_en" in tool_args:
                response(message_id, {"content": [{"type": "text", "text": "selection-only focus arguments must be stripped"}], "isError": True})
            else:
                page_result = (
                    selected_english_page_result()
                    if tool_args.get("url") == "https://example.test/en/terms"
                    else selected_page_result()
                )
                response(message_id, {"content": [{"type": "text", "text": json.dumps(page_result, ensure_ascii=False)}], "_meta": {"trace": "RAW_NOISE_META_SENTINEL"}})
        else:
            error(message_id, -32602, f"unknown mock tool: {name}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
