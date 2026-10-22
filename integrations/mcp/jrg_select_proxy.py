#!/usr/bin/env python3
"""Bounded stdio JSONL MCP proxy that selects evidence from mapped tools."""

from __future__ import annotations

import argparse
import json
import math
import os
import selectors
import signal
import subprocess
import sys
import time
import unicodedata
from collections import deque
from dataclasses import dataclass
from typing import Any


MAX_FRAME = 4 * 1024 * 1024
MAX_QUEUED_BYTES = 4 * 1024 * 1024
IO_CHUNK = 64 * 1024


def stop_process_group(child: subprocess.Popen[bytes]) -> None:
    """Stop our isolated group even if its original leader has already exited."""
    # One group signal also stops descendants when their leader is already gone.
    try:
        os.killpg(child.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    try:
        child.wait(timeout=0.6)
    except subprocess.TimeoutExpired:
        pass


class FrameTooLarge(Exception):
    pass


class StreamEnded(Exception):
    pass


@dataclass
class ProxyFailure(Exception):
    message: str
    code: int = -32000
    fatal: bool = False

    def __str__(self) -> str:
        return self.message


class JsonLineReader:
    """Read bounded newline-delimited frames without blocking on partial lines."""

    def __init__(self, fd: int) -> None:
        self.fd = fd
        self.buffer = bytearray()
        self.eof = False

    def pop(self) -> bytes | None:
        newline = self.buffer.find(b"\n")
        if newline >= 0:
            if newline > MAX_FRAME:
                raise FrameTooLarge
            frame = bytes(self.buffer[:newline])
            del self.buffer[: newline + 1]
            return frame
        if len(self.buffer) > MAX_FRAME:
            raise FrameTooLarge
        if self.eof:
            if self.buffer:
                raise StreamEnded("stream ended with an incomplete JSONL frame")
            return None
        return None

    def read_available(self) -> bool:
        """Read one ready chunk. Return false after EOF."""
        try:
            data = os.read(self.fd, IO_CHUNK)
        except BlockingIOError:
            return True
        except InterruptedError:
            return True
        if not data:
            self.eof = True
            return False
        self.buffer.extend(data)
        # Check the first complete frame immediately. pop() applies the same cap
        # while retaining later frames for the caller.
        if b"\n" not in self.buffer and len(self.buffer) > MAX_FRAME:
            raise FrameTooLarge
        first_newline = self.buffer.find(b"\n")
        if first_newline > MAX_FRAME:
            raise FrameTooLarge
        return True


def compact_json(value: Any) -> bytes:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode("utf-8")


def has_searchable_terms(text: str) -> bool:
    normalized = unicodedata.normalize("NFKC", text)
    return any(character.isalnum() for character in normalized)


def parse_message(frame: bytes, *, where: str) -> dict[str, Any]:
    try:
        value = json.loads(frame.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ProxyFailure(f"{where} returned invalid JSON") from error
    if not isinstance(value, dict) or value.get("jsonrpc") != "2.0":
        raise ProxyFailure(f"{where} returned an invalid JSON-RPC message")
    if where == "upstream" and "method" in value:
        if not isinstance(value["method"], str) or "result" in value or "error" in value:
            raise ProxyFailure("upstream returned an invalid JSON-RPC request or notification")
    if where == "upstream" and "method" not in value:
        if "id" not in value or ("result" in value) == ("error" in value):
            raise ProxyFailure("upstream response must contain exactly one result or error")
        if "error" in value:
            error = value["error"]
            if (not isinstance(error, dict) or type(error.get("code")) is not int
                    or not isinstance(error.get("message"), str)):
                raise ProxyFailure("upstream returned an invalid JSON-RPC error")
    return value


def error_message(request_id: Any, code: int, message: str) -> dict[str, Any]:
    return {"jsonrpc": "2.0", "id": request_id, "error": {"code": code, "message": message}}


def is_request(message: dict[str, Any]) -> bool:
    return "id" in message


class SelectionProxy:
    def __init__(self, args: argparse.Namespace) -> None:
        self.args = args
        self.client = JsonLineReader(0)
        self.pending_client: deque[bytes] = deque()
        self.pending_bytes = 0
        self.child: subprocess.Popen[bytes] | None = None
        self.upstream: JsonLineReader | None = None
        self.stop_requested = False
        self.exit_code = 0
        self.selection_active = False
        self.active_selection_id: Any = None

        os.set_blocking(0, False)
        self._start_upstream()

    def _start_upstream(self) -> None:
        try:
            self.child = subprocess.Popen(
                self.args.upstream,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL,
                bufsize=0,
                start_new_session=True,
            )
        except OSError as error:
            raise ProxyFailure(f"could not start upstream MCP server: {error.strerror}", fatal=True) from error
        assert self.child.stdin is not None and self.child.stdout is not None
        os.set_blocking(self.child.stdin.fileno(), False)
        os.set_blocking(self.child.stdout.fileno(), False)
        self.upstream = JsonLineReader(self.child.stdout.fileno())

    def shutdown_upstream(self) -> None:
        child = self.child
        self.child = None
        self.upstream = None
        if child is None:
            return
        stop_process_group(child)
        for stream in (child.stdin, child.stdout):
            if stream is not None:
                try:
                    stream.close()
                except OSError:
                    pass

    def _check_signal(self) -> None:
        if self.stop_requested:
            raise ProxyFailure("proxy interrupted", code=-32800, fatal=True)

    def _write_client(self, message: dict[str, Any]) -> None:
        encoded = compact_json(message)
        if len(encoded) > MAX_FRAME:
            if "id" in message:
                message = error_message(message["id"], -32000, "proxy response exceeds 4 MiB")
                encoded = compact_json(message)
            else:
                return
        payload = encoded + b"\n"
        offset = 0
        while offset < len(payload):
            try:
                written = os.write(1, payload[offset:])
            except BrokenPipeError:
                self.stop_requested = True
                return
            except InterruptedError:
                continue
            if written <= 0:
                self.stop_requested = True
                return
            offset += written

    def _write_upstream(self, payload: bytes, deadline: float) -> None:
        child = self.child
        if child is None or child.poll() is not None or child.stdin is None:
            raise ProxyFailure("upstream MCP server is unavailable", fatal=True)
        if len(payload) > MAX_FRAME:
            raise ProxyFailure("upstream request exceeds 4 MiB", code=-32600)
        frame = payload + b"\n"
        fd = child.stdin.fileno()
        offset = 0
        while offset < len(frame):
            self._check_signal()
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise ProxyFailure("upstream request timed out", code=-32001, fatal=True)
            with selectors.DefaultSelector() as selector:
                selector.register(fd, selectors.EVENT_WRITE)
                events = selector.select(min(remaining, 0.25))
            if not events:
                continue
            try:
                written = os.write(fd, frame[offset : offset + IO_CHUNK])
            except BrokenPipeError as error:
                raise ProxyFailure("upstream MCP server closed its input", fatal=True) from error
            except InterruptedError:
                continue
            if written <= 0:
                raise ProxyFailure("upstream MCP server closed its input", fatal=True)
            offset += written

    def _read_client_during_upstream_wait(self) -> None:
        while True:
            frame = self.client.pop()
            if frame is None:
                break
            try:
                message = parse_message(frame, where="client")
            except ProxyFailure as error:
                self._write_client(error_message(None, -32700, error.message))
                continue
            if not isinstance(message.get("method"), str):
                if is_request(message):
                    self._write_client(error_message(message.get("id"), -32600, "invalid JSON-RPC request"))
                continue
            if "id" not in message:
                params = message.get("params")
                if (self.selection_active and message.get("method") == "notifications/cancelled"
                        and isinstance(params, dict) and params.get("requestId") == self.active_selection_id):
                    raise ProxyFailure("selection request cancelled", code=-32800)
                # Notifications, including notifications/cancelled, are
                # forwarded immediately. The proxy does not handle callbacks.
                self._write_upstream(frame, time.monotonic() + self.args.timeout)
                continue
            if self.pending_bytes + len(frame) > MAX_QUEUED_BYTES:
                raise ProxyFailure("too many client requests queued while waiting for upstream", code=-32600, fatal=True)
            self.pending_client.append(frame)
            self.pending_bytes += len(frame)

    def _queue_client_chunk(self) -> None:
        alive = self.client.read_available()
        if not alive and not self.client.buffer:
            raise ProxyFailure("client closed the stdio connection", fatal=True)
        self._read_client_during_upstream_wait()

    def _wait_for_upstream(self, request_id: Any, deadline: float) -> dict[str, Any]:
        child = self.child
        upstream = self.upstream
        if child is None or upstream is None or child.stdout is None:
            raise ProxyFailure("upstream MCP server is unavailable", fatal=True)
        upstream_fd = child.stdout.fileno()
        client_fd = self.client.fd

        while True:
            self._check_signal()
            # The initial client read may already have buffered the next line
            # (commonly notifications/cancelled) before this wait began.
            self._read_client_during_upstream_wait()
            while True:
                try:
                    frame = upstream.pop()
                except FrameTooLarge as error:
                    raise ProxyFailure("upstream JSON-RPC frame exceeds 4 MiB", fatal=True) from error
                if frame is None:
                    break
                try:
                    message = parse_message(frame, where="upstream")
                except ProxyFailure as error:
                    raise ProxyFailure(error.message, error.code, fatal=True) from error
                if "id" not in message:
                    # JSON-RPC notifications do not have a response channel.
                    continue
                if isinstance(message.get("method"), str):
                    method = message["method"]
                    raise ProxyFailure(
                        f"unsupported upstream callback: {method}",
                        code=-32002,
                        fatal=True,
                    )
                if message.get("id") != request_id:
                    raise ProxyFailure("upstream returned a response with an unexpected id", fatal=True)
                return message

            if upstream.eof:
                raise ProxyFailure("upstream MCP server closed stdout before replying", fatal=True)
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise ProxyFailure("upstream request timed out", code=-32001, fatal=True)

            with selectors.DefaultSelector() as selector:
                selector.register(upstream_fd, selectors.EVENT_READ, "upstream")
                selector.register(client_fd, selectors.EVENT_READ, "client")
                events = selector.select(min(remaining, 0.25))
            for key, _ in events:
                if key.data == "upstream":
                    try:
                        if not upstream.read_available() and not upstream.buffer:
                            raise ProxyFailure("upstream MCP server closed stdout before replying", fatal=True)
                    except FrameTooLarge as error:
                        raise ProxyFailure("upstream JSON-RPC frame exceeds 4 MiB", fatal=True) from error
                else:
                    self._queue_client_chunk()

    def _read_client(self) -> bytes | None:
        while True:
            frame = self.client.pop()
            if frame is not None:
                return frame
            if self.client.eof:
                return None
            self._check_signal()
            with selectors.DefaultSelector() as selector:
                selector.register(self.client.fd, selectors.EVENT_READ)
                selector.select(0.25)
            if not self.client.read_available() and not self.client.buffer:
                return None

    def _mapped_call(
        self, request: dict[str, Any], adapter: str
    ) -> tuple[dict[str, Any], str, str | None]:
        params = request.get("params")
        if not isinstance(params, dict) or not isinstance(params.get("arguments"), dict):
            raise ProxyFailure("mapped tools/call arguments must be an object", code=-32602)
        original_arguments = params["arguments"]
        focus_value = original_arguments.get("jrg_focus")
        if "jrg_focus" in original_arguments and not isinstance(focus_value, str):
            raise ProxyFailure("jrg_focus must be a string", code=-32602)
        explicit_focus = focus_value.strip() if isinstance(focus_value, str) else ""

        if adapter == "markdown-page":
            if not explicit_focus:
                raise ProxyFailure("mapped fetch requires explicit jrg_focus", code=-32602)
            focus = explicit_focus
        else:
            focus = explicit_focus
            if not focus:
                for key in ("query", "q"):
                    candidate = original_arguments.get(key)
                    if isinstance(candidate, str) and candidate.strip():
                        focus = candidate.strip()
                        break
            if not focus:
                raise ProxyFailure("mapped search requires jrg_focus, query, or q", code=-32602)

        try:
            focus_bytes = focus.encode("utf-8")
        except UnicodeEncodeError as error:
            raise ProxyFailure("primary focus must be valid UTF-8", code=-32602) from error

        english_focus: str | None = None
        english_bytes: bytes | None = None
        if "jrg_focus_en" in original_arguments:
            english_value = original_arguments["jrg_focus_en"]
            if not isinstance(english_value, str):
                raise ProxyFailure("jrg_focus_en must be a string", code=-32602)
            english_focus = english_value.strip()
            try:
                english_bytes = english_focus.encode("utf-8")
            except UnicodeEncodeError as error:
                raise ProxyFailure("jrg_focus_en must be valid UTF-8", code=-32602) from error
            if not english_focus:
                raise ProxyFailure("jrg_focus_en must be nonempty", code=-32602)
            if not has_searchable_terms(english_focus):
                raise ProxyFailure("jrg_focus_en must contain searchable terms", code=-32602)
            if not has_searchable_terms(focus):
                raise ProxyFailure("primary focus must contain searchable terms", code=-32602)
            assert english_bytes is not None
            combined_bytes = len(focus_bytes) + 1 + len(english_bytes)
            if combined_bytes > 4096:
                raise ProxyFailure(
                    "combined query and expansions must be at most 4096 UTF-8 bytes",
                    code=-32602,
                )

        forwarded = dict(original_arguments)
        forwarded.pop("jrg_focus", None)
        forwarded.pop("jrg_focus_en", None)
        forwarded_params = dict(params)
        forwarded_params["arguments"] = forwarded
        forwarded_request = dict(request)
        forwarded_request["params"] = forwarded_params
        return forwarded_request, focus, english_focus

    def _wrap_tools_list(self, response: dict[str, Any]) -> dict[str, Any]:
        if "error" in response:
            return response
        result = response.get("result")
        if not isinstance(result, dict) or not isinstance(result.get("tools"), list):
            raise ProxyFailure("upstream tools/list result has no tools array")
        copied_result = dict(result)
        copied_tools: list[Any] = []
        mapped_descriptions = {
            self.args.search_tool: "Question used to select relevant search evidence; defaults to query or q.",
            self.args.page_tool: "Question used to select relevant page evidence; required for this tool.",
        }
        english_focus_description = (
            "Optional English equivalent for English-language evidence. "
            "Generate from the user's task before reading tool output. "
            "Preserve negation, versions, numbers, and identifiers; do not add answers or assume settings. "
            "Where unambiguous, include standard API terminology and a short equivalent form of numerical bounds "
            "(for example, zero or negative / non-positive). Keep the expression concise. Do not add facts."
        )
        for tool in result["tools"]:
            if not isinstance(tool, dict) or tool.get("name") not in mapped_descriptions:
                copied_tools.append(tool)
                continue
            name = tool["name"]
            schema = tool.get("inputSchema")
            if not isinstance(schema, dict):
                raise ProxyFailure(f"mapped tool {name} has no object inputSchema")
            properties = schema.get("properties", {})
            if not isinstance(properties, dict):
                raise ProxyFailure(f"mapped tool {name} has an invalid inputSchema properties field")
            required = schema.get("required", [])
            if not isinstance(required, list) or any(not isinstance(item, str) for item in required):
                raise ProxyFailure(f"mapped tool {name} has an invalid inputSchema required field")
            copied_schema = dict(schema)
            copied_properties = dict(properties)
            copied_properties["jrg_focus"] = {
                "type": "string",
                "description": mapped_descriptions[name],
            }
            copied_properties["jrg_focus_en"] = {
                "type": "string",
                "description": english_focus_description,
            }
            normalized_required = [item for item in required if item != "jrg_focus_en"]
            if name == self.args.page_tool and "jrg_focus" not in normalized_required:
                # Page selection cannot infer an intent from the fetch URL.
                normalized_required.append("jrg_focus")
            copied_schema["properties"] = copied_properties
            if normalized_required != required:
                copied_schema["required"] = normalized_required
            copied_tool = dict(tool)
            copied_tool["inputSchema"] = copied_schema
            # The upstream output is replaced by a SelectionResponse, so its
            # declared result shape would mislead MCP clients.
            copied_tool.pop("outputSchema", None)
            copied_tools.append(copied_tool)
        copied_result["tools"] = copied_tools
        return {"jsonrpc": "2.0", "id": response.get("id"), "result": copied_result}

    def _extract_object(self, tool_result: Any) -> dict[str, Any]:
        if not isinstance(tool_result, dict):
            raise ProxyFailure("mapped tool returned an invalid result object")
        content = tool_result.get("content")
        if "content" in tool_result:
            if not isinstance(content, list):
                raise ProxyFailure("mapped tool result mixed media is unsupported")
            if any(not isinstance(item, dict) or item.get("type") != "text" for item in content):
                raise ProxyFailure("mapped tool result mixed media is unsupported")
        structured = tool_result.get("structuredContent")
        if "structuredContent" in tool_result:
            if not isinstance(structured, dict):
                raise ProxyFailure("mapped tool result structuredContent must be a JSON object")
            return structured

        if not isinstance(content, list) or len(content) != 1:
            raise ProxyFailure("mapped tool result mixed media is unsupported; expected one text content item")
        item = content[0]
        if not isinstance(item, dict) or item.get("type") != "text" or not isinstance(item.get("text"), str):
            raise ProxyFailure("mapped tool result mixed media is unsupported; expected one text content item")
        try:
            value = json.loads(item["text"])
        except json.JSONDecodeError as error:
            raise ProxyFailure("mapped tool result sole text content is not valid JSON") from error
        if not isinstance(value, dict):
            raise ProxyFailure("mapped tool result text must contain a JSON object")
        return value

    def _run_selector(
        self,
        adapter: str,
        focus: str,
        source: dict[str, Any],
        deadline: float,
        request_id: Any,
        english_focus: str | None,
    ) -> dict[str, Any]:
        selection_request = {"query": focus, "max_tokens": self.args.max_tokens, "input": source}
        if english_focus is not None:
            selection_request["query_expansions"] = [english_focus]
        payload = compact_json(selection_request)
        if len(payload) > MAX_FRAME:
            raise ProxyFailure("selection input exceeds 4 MiB")
        command = [self.args.selector, "--adapter", adapter, "--query-rules", "verified", "-"]
        try:
            process = subprocess.Popen(
                command,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL,
                bufsize=0,
                start_new_session=True,
            )
        except OSError as error:
            raise ProxyFailure("could not start jrg-select") from error
        assert process.stdin is not None and process.stdout is not None
        input_fd = process.stdin.fileno()
        output_fd = process.stdout.fileno()
        os.set_blocking(input_fd, False)
        os.set_blocking(output_fd, False)
        pending = memoryview(payload + b"\n")
        offset = 0
        output = bytearray()
        output_eof = False
        input_open = True
        self.selection_active = True
        self.active_selection_id = request_id
        try:
            while input_open or not output_eof or process.poll() is None:
                self._check_signal()
                self._read_client_during_upstream_wait()
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise ProxyFailure("selection timed out", code=-32001)
                with selectors.DefaultSelector() as selector:
                    selector.register(self.client.fd, selectors.EVENT_READ, "client")
                    if input_open:
                        selector.register(input_fd, selectors.EVENT_WRITE, "input")
                    if not output_eof:
                        selector.register(output_fd, selectors.EVENT_READ, "output")
                    events = selector.select(min(remaining, 0.25))
                for key, _ in events:
                    if key.data == "client":
                        self._queue_client_chunk()
                    elif key.data == "input":
                        try:
                            written = os.write(input_fd, pending[offset : offset + IO_CHUNK])
                        except BrokenPipeError:
                            input_open = False
                            process.stdin.close()
                            continue
                        except InterruptedError:
                            continue
                        offset += written
                        if offset >= len(pending):
                            input_open = False
                            process.stdin.close()
                    else:
                        try:
                            chunk = os.read(output_fd, IO_CHUNK)
                        except BlockingIOError:
                            continue
                        except InterruptedError:
                            continue
                        if not chunk:
                            output_eof = True
                        else:
                            output.extend(chunk)
                            if len(output) > MAX_FRAME:
                                raise ProxyFailure("jrg-select output exceeds 4 MiB")
                if process.poll() is not None and output_eof and not input_open:
                    break
            remaining = max(0.0, deadline - time.monotonic())
            try:
                status = process.wait(timeout=remaining)
            except subprocess.TimeoutExpired as error:
                raise ProxyFailure("selection timed out", code=-32001) from error
        finally:
            stop_process_group(process)
            self.selection_active = False
            self.active_selection_id = None
            for stream in (process.stdin, process.stdout):
                if stream is not None:
                    try:
                        stream.close()
                    except OSError:
                        pass
        if status != 0:
            raise ProxyFailure(f"selection failed: jrg-select exited with status {status}")
        try:
            selected = json.loads(output.decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError) as error:
            raise ProxyFailure("selection failed: jrg-select returned invalid JSON") from error
        if not isinstance(selected, dict):
            raise ProxyFailure("selection failed: jrg-select returned a non-object result")
        return selected

    def _rewrite_mapped_response(
        self,
        response: dict[str, Any],
        adapter: str,
        focus: str,
        deadline: float,
        english_focus: str | None,
    ) -> dict[str, Any]:
        if "error" in response:
            return response
        result = response.get("result")
        if not isinstance(result, dict):
            raise ProxyFailure("mapped tool returned an invalid result object")
        if result.get("isError") is True:
            content = result.get("content", [])
            if not isinstance(content, list):
                content = [{"type": "text", "text": "upstream tool reported an error"}]
            clean = {"content": content, "isError": True}
            wrapped = {"jsonrpc": "2.0", "id": response.get("id"), "result": clean}
            if len(compact_json(wrapped)) > MAX_FRAME:
                wrapped["result"] = {
                    "content": [{"type": "text", "text": "upstream tool reported an error"}],
                    "isError": True,
                }
            return wrapped
        source = self._extract_object(result)
        selected = self._run_selector(
            adapter, focus, source, deadline, response.get("id"), english_focus
        )
        clean = {"content": [{"type": "text", "text": compact_json(selected).decode("utf-8")}]}
        wrapped = {"jsonrpc": "2.0", "id": response.get("id"), "result": clean}
        if len(compact_json(wrapped)) > MAX_FRAME:
            raise ProxyFailure("selected result exceeds 4 MiB")
        return wrapped

    def _handle_request(self, frame: bytes) -> None:
        try:
            request = parse_message(frame, where="client")
        except ProxyFailure as failure:
            self._write_client(error_message(None, -32700, failure.message))
            self.exit_code = 2
            self.shutdown_upstream()
            self.stop_requested = True
            return

        if not isinstance(request.get("method"), str):
            if is_request(request):
                self._write_client(error_message(request.get("id"), -32600, "invalid JSON-RPC request"))
            return
        if not is_request(request):
            if self.child is None:
                return
            self._write_upstream(frame, time.monotonic() + self.args.timeout)
            return

        request_id = request["id"]
        mapped: tuple[str, str, str | None] | None = None
        outgoing = request
        try:
            if request["method"] == "tools/call":
                params = request.get("params")
                name = params.get("name") if isinstance(params, dict) else None
                if name == self.args.search_tool:
                    outgoing, focus, english_focus = self._mapped_call(request, "search-results")
                    mapped = ("search-results", focus, english_focus)
                elif name == self.args.page_tool:
                    outgoing, focus, english_focus = self._mapped_call(request, "markdown-page")
                    mapped = ("markdown-page", focus, english_focus)
            if self.child is None:
                raise ProxyFailure("upstream MCP server is unavailable", fatal=True)
            deadline = time.monotonic() + self.args.timeout
            self._write_upstream(compact_json(outgoing), deadline)
            response = self._wait_for_upstream(request_id, deadline)
            if request["method"] == "tools/list":
                response = self._wrap_tools_list(response)
            elif mapped is not None:
                response = self._rewrite_mapped_response(
                    response, mapped[0], mapped[1], deadline, mapped[2]
                )
            if len(compact_json(response)) > MAX_FRAME:
                raise ProxyFailure("upstream response exceeds 4 MiB")
            self._write_client(response)
        except ProxyFailure as failure:
            self._write_client(error_message(request_id, failure.code, failure.message))
            if failure.fatal:
                self.shutdown_upstream()
        except FrameTooLarge:
            self._write_client(error_message(request_id, -32600, "client JSON-RPC frame exceeds 4 MiB"))
            self.exit_code = 2
            self.shutdown_upstream()
            self.stop_requested = True
        except StreamEnded:
            self._write_client(error_message(request_id, -32600, "client closed with an incomplete JSON-RPC frame"))
            self.exit_code = 2
            self.shutdown_upstream()
            self.stop_requested = True

    def _next_request_frame(self) -> bytes | None:
        if self.pending_client:
            frame = self.pending_client.popleft()
            self.pending_bytes -= len(frame)
            return frame
        return self._read_client()

    def run(self) -> int:
        while not self.stop_requested:
            try:
                frame = self._next_request_frame()
                if frame is None:
                    break
                self._handle_request(frame)
            except FrameTooLarge:
                self._write_client(error_message(None, -32600, "client JSON-RPC frame exceeds 4 MiB"))
                self.exit_code = 2
                break
            except StreamEnded:
                self._write_client(error_message(None, -32700, "client closed with an incomplete JSON-RPC frame"))
                self.exit_code = 2
                break
            except ProxyFailure as failure:
                self._write_client(error_message(None, failure.code, failure.message))
                self.exit_code = 2
                if failure.fatal:
                    break
        self.shutdown_upstream()
        return self.exit_code


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--selector", required=True, help="path to the jrg-select executable")
    parser.add_argument("--max-tokens", type=int, default=1000)
    parser.add_argument("--timeout", type=float, default=10.0, help="per-request timeout in seconds")
    parser.add_argument("--search-tool", default="search")
    parser.add_argument("--page-tool", default="fetch")
    parser.add_argument("--upstream", nargs=argparse.REMAINDER, required=True, help="upstream stdio MCP command and arguments")
    args = parser.parse_args(argv)
    if args.upstream and args.upstream[0] == "--":
        args.upstream = args.upstream[1:]
    if not args.upstream:
        parser.error("--upstream requires a command")
    if not 64 <= args.max_tokens <= 26_000:
        parser.error("--max-tokens must be between 64 and 26000")
    if not math.isfinite(args.timeout) or args.timeout <= 0:
        parser.error("--timeout must be a finite positive number")
    if not args.search_tool or not args.page_tool or args.search_tool == args.page_tool:
        parser.error("--search-tool and --page-tool must be nonempty and distinct")
    if not os.path.isfile(args.selector) or not os.access(args.selector, os.X_OK):
        parser.error("--selector must name an executable file")
    return args


def main(argv: list[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    proxy: SelectionProxy | None = None

    def request_stop(_signum: int, _frame: Any) -> None:
        if proxy is not None:
            proxy.stop_requested = True

    signal.signal(signal.SIGTERM, request_stop)
    signal.signal(signal.SIGINT, request_stop)
    try:
        proxy = SelectionProxy(args)
        return proxy.run()
    except ProxyFailure as failure:
        sys.stderr.write(f"jrg-select-proxy: {failure.message}\n")
        if proxy is not None:
            proxy.shutdown_upstream()
        return 2
    except BrokenPipeError:
        if proxy is not None:
            proxy.shutdown_upstream()
        return 0


if __name__ == "__main__":
    raise SystemExit(main())
