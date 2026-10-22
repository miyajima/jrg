#!/usr/bin/env python3
"""Black-box stdio MCP selection-proxy tests using a local mock peer."""

from __future__ import annotations

import json
import os
import selectors
import signal
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PROXY = ROOT / "integrations/mcp/jrg_select_proxy.py"
MOCK = ROOT / "tests/fixtures/selection_mock_mcp.py"
MAX_FRAME = 4 * 1024 * 1024
SELECTOR = Path(os.environ.get("JRG_SELECT_BIN", ROOT / "target/debug/jrg-select"))


def wire(value: object) -> bytes:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode() + b"\n"


def wire_escaped(value: object) -> bytes:
    return json.dumps(value, ensure_ascii=True, separators=(",", ":")).encode("utf-8") + b"\n"


class ProxySession:
    def __init__(
        self,
        *,
        mode: str = "normal",
        timeout: float = 3.0,
        pid_file: Path | None = None,
        selector: Path | None = None,
        upstream_script: Path | None = None,
    ) -> None:
        command = [
            sys.executable,
            str(PROXY),
            "--selector",
            str(selector or SELECTOR),
            "--max-tokens",
            "1000",
            "--timeout",
            str(timeout),
            "--search-tool",
            "search",
            "--page-tool",
            "fetch",
            "--upstream",
            sys.executable,
            str(upstream_script or MOCK),
            "--mode",
            mode,
        ]
        if pid_file is not None:
            command.extend(["--pid-file", str(pid_file)])
        self.proc = subprocess.Popen(
            command,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            bufsize=0,
            start_new_session=True,
        )
        assert self.proc.stdin is not None and self.proc.stdout is not None
        self._stdout_fd = self.proc.stdout.fileno()
        self._pending = bytearray()

    def send(self, value: object) -> None:
        assert self.proc.stdin is not None
        self.proc.stdin.write(wire(value))
        self.proc.stdin.flush()

    def send_escaped(self, value: object) -> None:
        assert self.proc.stdin is not None
        self.proc.stdin.write(wire_escaped(value))
        self.proc.stdin.flush()

    def send_raw_in_background(self, data: bytes) -> threading.Thread:
        assert self.proc.stdin is not None
        stream = self.proc.stdin

        def write_all() -> None:
            try:
                for offset in range(0, len(data), 16 * 1024):
                    stream.write(data[offset : offset + 16 * 1024])
                    stream.flush()
            except (BrokenPipeError, OSError):
                pass

        worker = threading.Thread(target=write_all, daemon=True)
        worker.start()
        return worker

    def receive(self, timeout: float = 3.0) -> dict:
        deadline = time.monotonic() + timeout
        while b"\n" not in self._pending:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise AssertionError("timed out waiting for proxy JSON-RPC response")
            with selectors.DefaultSelector() as selector:
                selector.register(self._stdout_fd, selectors.EVENT_READ)
                if not selector.select(remaining):
                    raise AssertionError("timed out waiting for proxy JSON-RPC response")
            chunk = os.read(self._stdout_fd, 64 * 1024)
            if not chunk:
                raise AssertionError(
                    f"proxy closed stdout before a response (exit={self.proc.poll()})"
                )
            self._pending.extend(chunk)
            if len(self._pending) > MAX_FRAME:
                raise AssertionError("proxy response exceeded the 4 MiB frame limit")
        frame, _, rest = self._pending.partition(b"\n")
        self._pending = bytearray(rest)
        return json.loads(frame)

    def close(self) -> None:
        if self.proc.poll() is None:
            try:
                if self.proc.stdin is not None:
                    self.proc.stdin.close()
            except OSError:
                pass
            try:
                self.proc.wait(timeout=0.5)
            except subprocess.TimeoutExpired:
                try:
                    os.killpg(self.proc.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                try:
                    self.proc.wait(timeout=0.5)
                except subprocess.TimeoutExpired:
                    try:
                        os.killpg(self.proc.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                    self.proc.wait(timeout=1)
        if self.proc.stdout is not None:
            self.proc.stdout.close()

    def __enter__(self) -> "ProxySession":
        return self

    def __exit__(self, *_: object) -> None:
        self.close()


def request(method: str, request_id: str, params: dict | None = None) -> dict:
    message = {"jsonrpc": "2.0", "id": request_id, "method": method}
    if params is not None:
        message["params"] = params
    return message


def call(tool: str, request_id: str, arguments: dict) -> dict:
    return request("tools/call", request_id, {"name": tool, "arguments": arguments})


def call_result(response: dict) -> dict:
    assert "error" not in response, response
    return response["result"]


def selected_result(response: dict) -> dict:
    result = call_result(response)
    assert not result.get("isError"), result
    content = result.get("content")
    assert isinstance(content, list) and len(content) == 1, result
    assert content[0].get("type") == "text", result
    return json.loads(content[0]["text"])


def assert_error(response: dict, phrase: str) -> None:
    assert "error" in response, response
    assert phrase.lower() in response["error"]["message"].lower(), response


def assert_process_gone(pid: int) -> None:
    for _ in range(50):
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            return
        time.sleep(0.02)
    raise AssertionError(f"upstream child {pid} is still running")


def write_selector_capture_wrapper(directory: Path, request_log: Path) -> Path:
    wrapper = directory / "selector-capture-wrapper"
    wrapper.write_text(
        f"#!{sys.executable}\n"
        "import subprocess, sys\n"
        "from pathlib import Path\n"
        "payload = sys.stdin.buffer.read()\n"
        f"with Path({str(request_log)!r}).open('ab') as log:\n"
        "    log.write(payload)\n"
        f"Path({str(request_log) + '.argv'!r}).write_text(__import__('json').dumps(sys.argv[1:]))\n"
        f"result = subprocess.run([{str(SELECTOR)!r}, *sys.argv[1:]], "
        "input=payload, stdout=subprocess.PIPE, check=False)\n"
        "sys.stdout.buffer.write(result.stdout)\n"
        "raise SystemExit(result.returncode)\n",
        encoding="utf-8",
    )
    wrapper.chmod(0o700)
    return wrapper


class SelectionProxyTests(unittest.TestCase):
    def setUp(self) -> None:
        if not PROXY.is_file():
            self.fail(f"selection proxy implementation is missing: {PROXY}")
        if not SELECTOR.is_file():
            self.fail(f"jrg-select binary is missing: {SELECTOR}")

    def test_initialize_notification_and_tool_schema_are_forwarded_and_wrapped(self) -> None:
        with ProxySession() as session:
            session.send(request("initialize", "init-1", {"protocolVersion": "2025-03-26"}))
            init = session.receive()
            self.assertEqual(init["id"], "init-1")
            self.assertEqual(init["result"]["serverInfo"]["name"], "selection-mock")

            session.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
            session.send(request("tools/list", "list-1"))
            listing = session.receive()["result"]["tools"]
            tools = {tool["name"]: tool for tool in listing}
            for name in ("search", "fetch"):
                tool = tools[name]
                focus = tool["inputSchema"]["properties"]["jrg_focus"]
                self.assertEqual(focus["type"], "string")
                english_focus = tool["inputSchema"]["properties"]["jrg_focus_en"]
                self.assertEqual(english_focus["type"], "string")
                self.assertNotIn("jrg_focus_en", tool["inputSchema"].get("required", []))
                description = english_focus["description"].lower()
                for term in ("negation", "version", "numbers", "identifiers", "facts"):
                    self.assertIn(term, description)
                self.assertNotIn("outputSchema", tool)
            self.assertIn("outputSchema", tools["other"])
            self.assertEqual(tools["search"]["inputSchema"]["required"], ["query"])
            self.assertEqual(tools["fetch"]["inputSchema"]["required"], ["url", "jrg_focus"])

    def test_bilingual_focus_selects_english_evidence_and_keeps_japanese_query(self) -> None:
        with tempfile.TemporaryDirectory() as directory_name:
            directory = Path(directory_name)
            request_log = directory / "selector-requests.jsonl"
            wrapper = write_selector_capture_wrapper(directory, request_log)
            with ProxySession(selector=wrapper) as session:
                session.send(request("initialize", "init-bilingual"))
                session.receive()

                session.send(call("search", "search-single", {
                    "query": "ordinary",
                    "jrg_focus": "解約時の違約金",
                }))
                single = selected_result(session.receive())
                self.assertEqual(single["documents"][0]["id"], "terms")

                japanese_focus = "契約を解約する期限と手順"
                english_focus = "cancellation subscription form 30 days before renewal contract version 3.2"
                session.send(call("search", "search-bilingual", {
                    "query": "bilingual",
                    "jrg_focus": japanese_focus,
                    "jrg_focus_en": english_focus,
                }))
                bilingual = selected_result(session.receive())
                self.assertEqual(bilingual["documents"][0]["id"], "english-terms")
                serialized = json.dumps(bilingual, ensure_ascii=False)
                self.assertIn(
                    "To request cancellation of a subscription, submit the form at least 30 days before renewal. Contract version: 3.2.",
                    serialized,
                )

                session.send(call("fetch", "fetch-bilingual", {
                    "url": "https://example.test/en/terms",
                    "jrg_focus": "違約金はかかりますか",
                    "jrg_focus_en": "cancellation fee 30 days before renewal",
                }))
                fetched = selected_result(session.receive())
                self.assertEqual(fetched["documents"][0]["id"], "english-terms")
                self.assertIn("Cancellation fees are waived", json.dumps(fetched, ensure_ascii=False))

            selector_requests = [json.loads(line) for line in request_log.read_bytes().splitlines()]
            self.assertEqual(len(selector_requests), 3)
            self.assertEqual(selector_requests[0]["query"], "解約時の違約金")
            self.assertNotIn("query_expansions", selector_requests[0])
            self.assertEqual(selector_requests[1]["query"], japanese_focus)
            self.assertEqual(selector_requests[1]["query_expansions"], [english_focus])
            self.assertEqual(selector_requests[2]["query"], "違約金はかかりますか")
            self.assertEqual(
                selector_requests[2]["query_expansions"],
                ["cancellation fee 30 days before renewal"],
            )

    def test_proxy_pins_verified_policy_even_for_unapproved_aliases(self) -> None:
        with tempfile.TemporaryDirectory() as directory_name:
            directory = Path(directory_name)
            request_log = directory / "selector-requests.jsonl"
            wrapper = write_selector_capture_wrapper(directory, request_log)
            with ProxySession(selector=wrapper) as session:
                session.send(request("initialize", "init-verified"))
                session.receive()
                session.send(call("search", "verified", {"query":"ordinary", "jrg_focus":"契約", "jrg_focus_en":"zero or negative"}))
                selected_result(session.receive())
            argv = json.loads(Path(str(request_log) + ".argv").read_text())
            self.assertEqual(argv, ["--adapter", "search-results", "--query-rules", "verified", "-"])

    def test_invalid_english_focus_is_rejected_before_upstream(self) -> None:
        cases = [
            ({"query": "lookup", "jrg_focus": "契約", "jrg_focus_en": 7}, "string"),
            ({"query": "lookup", "jrg_focus": "契約", "jrg_focus_en": " \t "}, "nonempty"),
            ({"query": "lookup", "jrg_focus": "契約", "jrg_focus_en": "---"}, "searchable"),
            ({"query": "lookup", "jrg_focus": "日" * 1365, "jrg_focus_en": "terms"}, "4096"),
        ]
        with ProxySession() as session:
            session.send(request("initialize", "init-invalid-english"))
            session.receive()
            for index, (arguments, expected) in enumerate(cases):
                session.send(call("search", f"invalid-english-{index}", arguments))
                assert_error(session.receive(), expected)

            surrogate_cases = [
                ({"query": "lookup", "jrg_focus": "契約\ud800", "jrg_focus_en": "terms"}, "primary focus"),
                ({"query": "lookup", "jrg_focus": "契約", "jrg_focus_en": "terms\ud800"}, "jrg_focus_en"),
                ({"query": "lookup", "jrg_focus": "契約\ud800"}, "primary focus"),
            ]
            for index, (arguments, field_name) in enumerate(surrogate_cases):
                session.send_escaped(call("search", f"invalid-surrogate-{index}", arguments))
                response = session.receive()
                assert_error(response, "UTF-8")
                self.assertEqual(response["error"]["code"], -32602)
                self.assertIn(field_name, response["error"]["message"])

            session.send(call("search", "english-boundary", {
                "query": "boundary",
                "jrg_focus": "x" * 4092,
                "jrg_focus_en": "abc",
            }))
            boundary_result = selected_result(session.receive())
            self.assertEqual(boundary_result["documents"], [])

            session.send(call("probe", "probe-invalid-english", {}))
            probe = call_result(session.receive())
            calls = json.loads(probe["content"][0]["text"])["calls"]
            self.assertEqual(calls, ["search", "probe"])

    def test_search_and_fetch_return_only_selected_json_across_sequential_calls(self) -> None:
        with ProxySession() as session:
            session.send(request("initialize", "init-2"))
            session.receive()
            session.send(call("search", "search-1", {"query": "unrelated", "jrg_focus": "解約 違約金"}))
            searched = session.receive()
            self.assertEqual(searched["id"], "search-1")
            selected_search = selected_result(searched)
            self.assertEqual(selected_search["documents"][0]["id"], "terms")
            self.assertEqual(selected_search["documents"][0]["url"], "https://example.test/terms")
            self.assertIn("解約時の違約金は無料です", json.dumps(selected_search, ensure_ascii=False))
            self.assertNotIn("RAW_NOISE_SEARCH_SENTINEL", json.dumps(searched, ensure_ascii=False))
            self.assertNotIn("RAW_NOISE_CONTENT_SENTINEL", json.dumps(searched, ensure_ascii=False))
            self.assertNotIn("RAW_NOISE_META_SENTINEL", json.dumps(searched, ensure_ascii=False))
            self.assertNotIn("RAW_NOISE_ENVELOPE_SENTINEL", json.dumps(searched, ensure_ascii=False))

            session.send(call("fetch", "fetch-1", {"url": "https://example.test/terms", "jrg_focus": "違約金"}))
            fetched = session.receive()
            self.assertEqual(fetched["id"], "fetch-1")
            selected_fetch = selected_result(fetched)
            self.assertEqual(selected_fetch["documents"][0]["url"], "https://example.test/terms")
            rendered = json.dumps(fetched, ensure_ascii=False)
            self.assertIn("違約金", rendered)
            self.assertNotIn("RAW_NOISE_PAGE_SENTINEL", rendered)
            self.assertNotIn("RAW_NOISE_META_SENTINEL", rendered)
            self.assertNotIn("structuredContent", fetched.get("result", {}))
            self.assertNotIn("_meta", fetched.get("result", {}))

    def test_search_uses_original_q_as_fallback_focus(self) -> None:
        with ProxySession() as session:
            session.send(request("initialize", "init-3"))
            session.receive()
            session.send(call("search", "search-q", {"q": "解約 違約金"}))
            response = session.receive()
            result = selected_result(response)
            self.assertEqual(result["documents"][0]["id"], "terms")

    def test_missing_fetch_focus_is_rejected_before_upstream_call(self) -> None:
        with ProxySession() as session:
            session.send(request("initialize", "init-4"))
            session.receive()
            session.send(call("fetch", "fetch-missing", {"url": "https://example.test/terms"}))
            assert_error(session.receive(), "explicit jrg_focus")
            session.send(call("probe", "probe-1", {}))
            observed = json.loads(call_result(session.receive())["content"][0]["text"])
            self.assertEqual(observed["calls"], ["probe"])

    def test_unknown_tool_is_forwarded_without_rewriting_its_result(self) -> None:
        with ProxySession() as session:
            session.send(request("initialize", "init-5"))
            session.receive()
            args = {"raw": "opaque request"}
            session.send(call("other", "other-1", args))
            response = session.receive()
            self.assertEqual(response["id"], "other-1")
            self.assertEqual(response["result"]["structuredContent"], {"passthrough": True, "arguments": args})
            self.assertEqual(response["result"]["_meta"], {"opaque": "unchanged"})

    def test_upstream_rpc_and_tool_errors_keep_error_semantics(self) -> None:
        with ProxySession() as session:
            session.send(request("initialize", "init-6"))
            session.receive()
            session.send(call("rpc_error", "rpc-error-1", {}))
            rpc_error = session.receive()
            self.assertEqual(rpc_error["error"]["code"], 1234)
            self.assertEqual(rpc_error["error"]["message"], "mock upstream failure")

            session.send(call("search", "tool-error-1", {"query": "tool-error"}))
            tool_error = session.receive()
            result = call_result(tool_error)
            self.assertTrue(result["isError"])
            self.assertEqual(result["content"][0]["text"], "mock tool failure")
            self.assertNotIn("structuredContent", result)
            self.assertNotIn("_meta", result)

    def test_malformed_and_mixed_results_fail_without_raw_fallback(self) -> None:
        with ProxySession() as session:
            session.send(request("initialize", "init-7"))
            session.receive()
            cases = [
                ({"query": "bad-structured"}, "structuredcontent"),
                ({"query": "bad-text"}, "json"),
                ({"query": "mixed-media"}, "media"),
                ({"query": "structured-mixed-media"}, "media"),
                ({"query": "bad-input"}, "selection"),
            ]
            for index, (arguments, expected) in enumerate(cases):
                request_id = f"bad-{index}"
                session.send(call("search", request_id, arguments))
                response = session.receive()
                assert_error(response, expected)
                serialized = json.dumps(response, ensure_ascii=False)
                self.assertNotIn("RAW_NOISE_MALFORMED_SENTINEL", serialized)
                self.assertNotIn("RAW_NOISE_STRUCTURED_MIXED_SENTINEL", serialized)
                self.assertNotIn("RAW_NOISE_SELECTION_SENTINEL", serialized)

    def test_client_cancellation_is_forwarded_and_upstream_is_reaped_on_close(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            pid_file = Path(directory) / "upstream.pid"
            with ProxySession(mode="cancel-aware", pid_file=pid_file) as session:
                session.send(request("initialize", "init-8"))
                session.receive()
                session.send(call("search", "cancel-me", {"query": "解約"}))
                session.send({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": "cancel-me"}})
                response = session.receive(timeout=2.0)
                self.assertEqual(response["error"]["code"], -32800)
                self.assertEqual(response["error"]["message"], "mock request cancelled")
            self.assertTrue(pid_file.is_file())
            assert_process_gone(int(pid_file.read_text()))

    def test_cancel_and_disconnect_during_selection_reap_the_selector(self) -> None:
        for action in ("cancel", "disconnect", "cancel_closed_stdout", "disconnect_closed_stdout"):
            with self.subTest(action=action), tempfile.TemporaryDirectory() as directory:
                pid_file = Path(directory) / "selector.pid"
                slow = Path(directory) / "slow-selector"
                close_stdout = "os.close(1)\n" if action.endswith("closed_stdout") else ""
                slow.write_text(f"#!{sys.executable}\nimport os,sys,time\nfrom pathlib import Path\nsys.stdin.buffer.read()\n{close_stdout}Path({str(pid_file)!r}).write_text(str(os.getpid()))\ntime.sleep(2)\n")
                slow.chmod(0o700)
                with ProxySession(selector=slow) as session:
                    session.send(call("search", "cancel-selection", {"query":"解約"}))
                    for _ in range(80):
                        if pid_file.exists():
                            break
                        time.sleep(0.02)
                    self.assertTrue(pid_file.exists())
                    if action.startswith("cancel"):
                        session.send({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":"cancel-selection"}})
                        response = session.receive(timeout=1.0)
                        self.assertEqual(response["error"]["code"], -32800)
                    else:
                        session.proc.stdin.close()
                        session.proc.wait(timeout=1.0)
                    assert_process_gone(int(pid_file.read_text()))

    def test_exited_wrapper_descendants_are_killed(self) -> None:
        for role in ("selector", "upstream"):
            with self.subTest(role=role), tempfile.TemporaryDirectory() as directory:
                pid_file = Path(directory) / "descendant.pid"
                wrapper = Path(directory) / "wrapper.py"
                wrapper.write_text(
                    f"#!{sys.executable}\nimport subprocess,sys\nfrom pathlib import Path\n"
                    "child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)'])\n"
                    f"Path({str(pid_file)!r}).write_text(str(child.pid))\n"
                )
                wrapper.chmod(0o700)
                descendant = None
                try:
                    kwargs = {"selector": wrapper} if role == "selector" else {"upstream_script": wrapper}
                    with ProxySession(timeout=1.0, **kwargs) as session:
                        session.send(call("search", "exited-wrapper", {"query": "解約"}))
                        for _ in range(100):
                            if pid_file.exists():
                                break
                            time.sleep(0.02)
                        self.assertTrue(pid_file.exists())
                        descendant = int(pid_file.read_text())
                        response = session.receive()
                        self.assertIn("error", response)
                    assert_process_gone(descendant)
                finally:
                    if descendant is not None:
                        try:
                            os.kill(descendant, signal.SIGKILL)
                        except ProcessLookupError:
                            pass

    def test_malformed_response_envelope_never_bypasses_selection(self) -> None:
        for query in ("bad-envelope", "bad-error", "bad-method-envelope"):
            with self.subTest(query=query), ProxySession() as session:
                session.send(call("search", "bad-envelope", {"query":query,"jrg_focus":"解約"}))
                response = session.receive()
                self.assertIn("error", response)
                self.assertNotIn("result", response)
                self.assertNotIn("RAW_BAD_ENVELOPE", json.dumps(response))

    def test_unsupported_upstream_callback_is_reported_and_child_is_cleaned_up(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            pid_file = Path(directory) / "upstream.pid"
            with ProxySession(mode="callback", pid_file=pid_file) as session:
                session.send(request("initialize", "init-9"))
                session.receive()
                session.send(call("search", "callback-1", {"query": "解約"}))
                response = session.receive()
                assert_error(response, "unsupported upstream callback")
            self.assertTrue(pid_file.is_file())
            assert_process_gone(int(pid_file.read_text()))

    def test_timeout_and_oversized_upstream_frame_never_return_raw_content(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            pid_file = Path(directory) / "timeout.pid"
            with ProxySession(mode="timeout", timeout=1.0, pid_file=pid_file) as session:
                session.send(request("initialize", "init-10"))
                session.receive()
                session.send(call("search", "timeout-1", {"query": "解約"}))
                response = session.receive(timeout=2.0)
                assert_error(response, "timed out")
                self.assertNotIn("RAW_TIMEOUT_SENTINEL", json.dumps(response))
            self.assertTrue(pid_file.is_file())
            assert_process_gone(int(pid_file.read_text()))

        with ProxySession(mode="oversized") as session:
            session.send(request("initialize", "init-11"))
            session.receive()
            session.send(call("search", "oversized-1", {"query": "解約"}))
            response = session.receive(timeout=3.0)
            assert_error(response, "4 mib")
            self.assertNotIn("RAW_OVERSIZED_SENTINEL", json.dumps(response))

    def test_oversized_client_frame_is_rejected(self) -> None:
        with ProxySession() as session:
            oversized = wire(request("tools/call", "too-large", {"name": "other", "arguments": {"text": "x" * MAX_FRAME}}))
            writer = session.send_raw_in_background(oversized)
            response = session.receive(timeout=5.0)
            writer.join(timeout=1.0)
            assert_error(response, "4 mib")


if __name__ == "__main__":
    unittest.main(verbosity=2)
