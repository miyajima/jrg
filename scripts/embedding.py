"""Offline EmbeddingGemma 2 candidate index using the local Ollama service.

The Unix socket is a small per-MCP bridge. The model itself is loaded by
Ollama only when the first search reaches /api/embed and is retained for five
minutes after the last embedding request.
"""

import argparse
import datetime
import hashlib
import json
import math
import os
import pathlib
import re
import signal
import socketserver
import sqlite3
import struct
import time
import urllib.parse
import urllib.request

MODEL = "google/embeddinggemma-2"
OLLAMA_MODEL = "embeddinggemma-2:270m-bf16-text"
OLLAMA_DIGEST = "afeb999480b2a5e48b590fc0583f8cbc7d206b74156feaead3a7840b81719295"
WIRE_REVISION = "914f7f89142e33e77833254d9c9b90c3cef7303b"
CACHE_REVISION = f"ollama:{OLLAMA_DIGEST}"
API_URL = "http://127.0.0.1:11434"
KEEP_ALIVE = "5m"
DIMENSION = 768
MAX_SOURCE = 4 * 1024 * 1024
MAX_TOTAL = 64 * 1024 * 1024
PROMPTS = {
    "Document": "title: none | text: ",
    "CodeRetrieval": "task: code retrieval | query: ",
}


class OllamaEncoder:
    """Lazy, digest-pinned adapter for the local Ollama embedding endpoint."""

    def __init__(self, api_url=API_URL, opener=None):
        parsed = urllib.parse.urlparse(api_url)
        if parsed.scheme != "http" or parsed.hostname not in {"127.0.0.1", "localhost", "::1"}:
            raise ValueError("Ollama embedding endpoint must be local HTTP")
        self.api_url = api_url.rstrip("/")
        self.opener = opener or urllib.request.build_opener(urllib.request.ProxyHandler({}))
        self.verified = False
        self.stats = {}

    def reset_stats(self):
        self.stats = {
            "provider": "ollama",
            "provider_model": OLLAMA_MODEL,
            "provider_digest": OLLAMA_DIGEST,
            "api_calls": 0,
            "embedded_inputs": 0,
            "load_duration_ns": 0,
            "total_duration_ns": 0,
        }

    def _request(self, endpoint, payload=None):
        url = self.api_url + endpoint
        if payload is None:
            request = urllib.request.Request(url, method="GET")
        else:
            request = urllib.request.Request(
                url,
                data=json.dumps(payload).encode("utf-8"),
                headers={"Content-Type": "application/json"},
                method="POST",
            )
        with self.opener.open(request, timeout=120) as response:
            raw = response.read(8 * 1024 * 1024 + 1)
        if len(raw) > 8 * 1024 * 1024:
            raise ValueError("Ollama response exceeds 8 MiB")
        return json.loads(raw.decode("utf-8"))

    def _verify_model(self):
        tags = self._request("/api/tags")
        models = tags.get("models") if isinstance(tags, dict) else None
        entry = next(
            (
                item
                for item in models or []
                if isinstance(item, dict)
                and item.get("name", item.get("model")) == OLLAMA_MODEL
            ),
            None,
        )
        if entry is None or entry.get("digest") != OLLAMA_DIGEST:
            raise ValueError("pinned Ollama model is missing or has changed")
        self.verified = True

    def encode(self, texts, prompt_name):
        if prompt_name not in PROMPTS:
            raise ValueError("unsupported EmbeddingGemma prompt")
        if not isinstance(texts, list) or not all(isinstance(text, str) for text in texts):
            raise ValueError("embedding inputs must be strings")
        if not texts:
            return []
        if not self.verified:
            self._verify_model()

        response = self._request(
            "/api/embed",
            {
                "model": OLLAMA_MODEL,
                "input": [PROMPTS[prompt_name] + text for text in texts],
                "keep_alive": KEEP_ALIVE,
            },
        )
        if not isinstance(response, dict) or response.get("model") != OLLAMA_MODEL:
            raise ValueError("Ollama returned a different embedding model")
        vectors = response.get("embeddings") if isinstance(response, dict) else None
        if not isinstance(vectors, list) or len(vectors) != len(texts):
            raise ValueError("invalid Ollama embedding batch")

        normalized = []
        for vector in vectors:
            if not isinstance(vector, list) or len(vector) != DIMENSION:
                raise ValueError("Ollama embedding must have 768 dimensions")
            values = [float(value) for value in vector]
            if not all(math.isfinite(value) for value in values):
                raise ValueError("Ollama embedding contains a non-finite value")
            norm = math.sqrt(sum(value * value for value in values))
            if norm == 0:
                raise ValueError("Ollama embedding has zero norm")
            normalized.append([value / norm for value in values])

        self.stats["api_calls"] = self.stats.get("api_calls", 0) + 1
        self.stats["embedded_inputs"] = self.stats.get("embedded_inputs", 0) + len(texts)
        self.stats["load_duration_ns"] = self.stats.get("load_duration_ns", 0) + int(
            response.get("load_duration", 0)
        )
        self.stats["total_duration_ns"] = self.stats.get("total_duration_ns", 0) + int(
            response.get("total_duration", 0)
        )
        return normalized


class Index:
    def __init__(self, cache, encoder, revision=CACHE_REVISION):
        self.encoder = encoder
        self.revision = revision
        self.db = sqlite3.connect(cache, timeout=30)
        self.db.execute("PRAGMA journal_mode=WAL")
        self.db.execute("PRAGMA busy_timeout=30000")
        self.db.execute("CREATE TABLE IF NOT EXISTS vectors(key TEXT PRIMARY KEY, vector BLOB)")
        self.db.commit()
        self.last_stats = {}

    def close(self):
        self.db.close()

    def search(self, files, query):
        started = time.monotonic()
        if hasattr(self.encoder, "reset_stats"):
            self.encoder.reset_stats()
        records = []
        missing = {}
        total = 0
        skipped = 0

        for name in sorted(set(files)):
            path = pathlib.Path(name)
            if path.is_symlink() or not path.is_file() or path.stat().st_size > MAX_SOURCE:
                skipped += 1
                continue
            raw = path.read_bytes()
            total += len(raw)
            if total > MAX_TOTAL:
                raise ValueError("index input exceeds 64 MiB")
            if b"\0" in raw:
                skipped += 1
                continue
            try:
                source = raw.decode("utf-8-sig")
            except UnicodeDecodeError:
                skipped += 1
                continue

            lines = source.splitlines()
            start = 0
            while start < len(lines):
                end = start
                size = 0
                while end < len(lines) and end - start < 80 and size + len(lines[end]) <= 4000:
                    size += len(lines[end]) + 1
                    end += 1
                if end == start:
                    end = start + 1
                content = "\n".join(lines[start:end])
                text = path.name + "\n" + content
                key = hashlib.sha256(
                    (self.revision + "|Document|windows80-4000-overlap20|" + text).encode()
                ).hexdigest()
                row = self.db.execute("SELECT vector FROM vectors WHERE key=?", (key,)).fetchone()
                vector = list(struct.unpack("<" + "f" * (len(row[0]) // 4), row[0])) if row else None
                if vector is None:
                    missing[key] = text
                records.append(
                    {
                        "path": str(path),
                        "start": start,
                        "end": end,
                        "lines": lines[start:end],
                        "key": key,
                        "vector": vector,
                    }
                )
                if end == len(lines):
                    break
                start = max(start + 1, end - 20)

        if not records:
            self.last_stats = {
                "files": len(set(files)),
                "chunks": 0,
                "encoded_documents": 0,
                "skipped": skipped,
                "index_seconds": time.monotonic() - started,
                "seconds": time.monotonic() - started,
                **getattr(self.encoder, "stats", {}),
            }
            return []

        encoded = {}
        items = list(missing.items())
        for offset in range(0, len(items), 16):
            batch = items[offset : offset + 16]
            vectors = self.encoder.encode([text for _, text in batch], prompt_name="Document")
            for (key, _), vector in zip(batch, vectors):
                encoded[key] = vector
                packed = struct.pack("<" + "f" * len(vector), *vector)
                self.db.execute("INSERT OR REPLACE INTO vectors VALUES(?,?)", (key, packed))
            self.db.commit()

        index_seconds = time.monotonic() - started
        query_vector = self.encoder.encode([query], prompt_name="CodeRetrieval")[0]
        query_norm = math.sqrt(sum(value * value for value in query_vector)) or 1.0
        for record in records:
            vector = record["vector"] if record["vector"] is not None else encoded[record["key"]]
            if len(vector) != len(query_vector) or not all(
                math.isfinite(value) for value in vector + query_vector
            ):
                raise ValueError("invalid embedding dimensions/values")
            norm = math.sqrt(sum(value * value for value in vector)) or 1.0
            record["score"] = sum(x * y for x, y in zip(vector, query_vector)) / (norm * query_norm)

        records.sort(key=lambda record: (-record["score"], record["path"], record["start"]))
        hits = []
        counts = {}
        words = set(re.findall(r"[\w]+", query.lower()))
        for record in records:
            if record["score"] <= 0 or counts.get(record["path"], 0) >= 2:
                continue
            counts[record["path"]] = counts.get(record["path"], 0) + 1
            lines = record["lines"]
            best = max(
                range(len(lines)),
                key=lambda i: (
                    len(words & set(re.findall(r"[\w]+", lines[i].lower()))),
                    bool(re.search(r"\b(fn|def|function)\s+\w+|\b(const|let)\s+\w+.*=>", lines[i])),
                    -i,
                ),
            )
            hits.append(
                {
                    "path": record["path"],
                    "line": record["start"] + best + 1,
                    "text": lines[best],
                    "score": record["score"],
                }
            )
            if len(hits) >= 24:
                break

        self.last_stats = {
            "files": len(set(files)),
            "chunks": len(records),
            "encoded_documents": len(missing),
            "skipped": skipped,
            "index_seconds": index_seconds,
            "seconds": time.monotonic() - started,
            **getattr(self.encoder, "stats", {}),
        }
        return hits


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", required=True)
    parser.add_argument("--socket", required=True)
    parser.add_argument("--log", required=True)
    parser.add_argument("--api-url", default=API_URL)
    args = parser.parse_args()

    os.umask(0o077)
    cache = pathlib.Path(args.cache)
    cache.parent.mkdir(parents=True, exist_ok=True)
    log_path = pathlib.Path(args.log)
    log_path.parent.mkdir(parents=True, exist_ok=True)

    def log(data):
        entry = {
            "timestamp_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds"),
            **data,
        }
        with log_path.open("a", encoding="utf-8") as stream:
            stream.write(json.dumps(entry, separators=(",", ":")) + "\n")

    index = Index(cache, OllamaEncoder(args.api_url))
    log(
        {
            "event": "bridge_ready",
            "model": MODEL,
            "wire_revision": WIRE_REVISION,
            "provider_model": OLLAMA_MODEL,
            "provider_digest": OLLAMA_DIGEST,
        }
    )

    class Handler(socketserver.StreamRequestHandler):
        def handle(self):
            started = time.monotonic()
            try:
                raw = self.rfile.readline(2 * 1024 * 1024 + 1)
                if len(raw) > 2 * 1024 * 1024:
                    raise ValueError("request exceeds 2 MiB")
                request = json.loads(raw)
                if (
                    request.get("version") != 1
                    or not isinstance(request.get("files"), list)
                    or len(request["files"]) > 8192
                    or not isinstance(request.get("query"), str)
                    or len(request["query"]) > 4096
                ):
                    raise ValueError("invalid request")
                hits = index.search(request["files"], request["query"])
                result = {
                    "version": 1,
                    "model": MODEL,
                    "revision": WIRE_REVISION,
                    "hits": hits,
                    "stats": index.last_stats,
                }
                log({"event": "search", **index.last_stats})
            except Exception as error:
                result = {"error": "embedding service error"}
                log(
                    {
                        "event": "error",
                        "error_type": type(error).__name__,
                        "seconds": time.monotonic() - started,
                    }
                )
            self.wfile.write(json.dumps(result, separators=(",", ":")).encode() + b"\n")

    def stop(signum, frame):
        raise SystemExit(0)

    signal.signal(signal.SIGTERM, stop)
    parent_pid = int(os.environ.get("JRG_EMBEDDING_PARENT_PID", os.getppid()))
    if os.getppid() != parent_pid:
        raise SystemExit("embedding bridge parent is no longer active")
    try:
        with socketserver.UnixStreamServer(args.socket, Handler) as server:
            server.timeout = 5
            try:
                while os.getppid() == parent_pid:
                    server.handle_request()
            finally:
                pathlib.Path(args.socket).unlink(missing_ok=True)
    finally:
        index.close()


if __name__ == "__main__":
    main()
