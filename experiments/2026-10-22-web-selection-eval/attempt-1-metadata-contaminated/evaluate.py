#!/usr/bin/env python3
"""Offline, frozen-corpus evaluation. Never calls a search/model service."""
import argparse
import copy
import hashlib
from html.parser import HTMLParser
import json
from pathlib import Path
import re
import statistics
import subprocess
import time

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parents[1]


def compact(value):
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def parse_search(raw):
    headers = list(re.finditer(r"^(.+) \((https?://[^\n)]+)\)\n[^\n]*cite[^\n]*\n", raw, re.M))
    if not headers:
        raise ValueError("search response has no recognized result headers")
    return {"results": [{"id": f"result-{i+1}", "url": match[2], "title": match[1],
                         "text": raw[match.end():headers[i+1].start() if i+1 < len(headers) else len(raw)]
                         if raw[match.end():headers[i+1].start() if i+1 < len(headers) else len(raw)].strip() else match[1]}
                        for i, match in enumerate(headers)]}


def evidence_score(documents, gold):
    return sum(any(doc["url"] == unit["url"] and
                   set(unit["block_ids"]) <= {b["id"] for b in doc["blocks"]}
                   for doc in documents) for unit in gold)


def original_order(documents, budget, count):
    selected = {}
    total = sum(len(d["blocks"]) for d in documents)

    def render(chosen):
        docs = [{**d, "blocks": [b for b in d["blocks"] if b["id"] in chosen.get(i, set())]}
                for i, d in enumerate(documents) if chosen.get(i)]
        result = {"documents": docs, "omitted_count": total - sum(len(d["blocks"]) for d in docs),
                  "duplicate_count": 0, "status": "selected" if docs else "budget_exhausted",
                  "tokens": 0, "max_tokens": budget}
        for _ in range(10):
            n = count(compact(result))
            if result["tokens"] == n:
                return result
            result["tokens"] = n
        raise RuntimeError("token count did not converge")

    for i, doc in enumerate(documents):
        by_id = {b["id"]: b for b in doc["blocks"]}
        for block in doc["blocks"]:
            if block["kind"] == "heading":
                continue
            pending, group = [block["id"]], set()
            while pending:
                key = pending.pop()
                if key in group:
                    continue
                group.add(key)
                parent = by_id[key].get("parent_id")
                if parent:
                    pending.append(parent)
                pending.extend(by_id[key].get("related_ids", []))
            trial = {k: set(v) for k, v in selected.items()}
            trial.setdefault(i, set()).update(group)
            if render(trial)["tokens"] <= budget:
                selected = trial
    result = render(selected)
    if result["tokens"] > budget:
        raise ValueError("budget cannot fit output envelope")
    return result


class Element:
    def __init__(self, tag="", attrs=()):
        self.tag, self.attrs, self.children = tag, dict(attrs), []

    def raw_text(self):
        return "".join(x.raw_text() if isinstance(x, Element) else x for x in self.children)

    def text(self):
        return re.sub(r"\s+", " ", self.raw_text()).strip()

    def descendants(self, tags):
        for child in self.children:
            if isinstance(child, Element):
                if child.tag in tags:
                    yield child
                yield from child.descendants(tags)


class PageParser(HTMLParser):
    VOID = {"area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr"}

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.root = Element()
        self.stack = [self.root]

    def handle_starttag(self, tag, attrs):
        node = Element(tag, attrs)
        self.stack[-1].children.append(node)
        if tag not in self.VOID:
            self.stack.append(node)

    def handle_endtag(self, tag):
        for i in range(len(self.stack)-1, 0, -1):
            if self.stack[i].tag == tag:
                del self.stack[i:]
                return

    def handle_data(self, text):
        self.stack[-1].children.append(text)


def markdown_from_html(html):
    parser = PageParser()
    parser.feed(html)
    root = next(parser.root.descendants({"article"}), None)
    if root is None:
        root = next(parser.root.descendants({"main"}), None)
    if root is None:
        root = next(parser.root.descendants({"body"}), parser.root)

    def render(node):
        if isinstance(node, str):
            return node
        tag = node.tag
        if tag in {"script", "style", "nav", "footer", "header", "aside", "noscript"}:
            return ""
        text = node.text()
        if re.fullmatch(r"h[1-6]", tag):
            return "\n\n" + "#" * int(tag[1]) + " " + text + "\n\n"
        if tag == "table":
            rows = ["| " + " | ".join(cell.text().replace("|", "\\|") for cell in row.children
                                     if isinstance(cell, Element) and cell.tag in {"td", "th"}) + " |"
                    for row in node.descendants({"tr"})]
            return "\n\n" + "\n".join(rows) + "\n\n"
        if tag == "pre":
            return "\n\n```\n" + node.raw_text() + "\n```\n\n"
        if tag == "li":
            return "\n- " + text + "\n"
        if tag == "p":
            return "\n\n" + text + "\n\n"
        if tag == "br":
            return "\n"
        result = "".join(render(x) for x in node.children)
        return "\n\n" + result + "\n\n" if tag in {"div", "section", "dl", "dt", "dd"} else result

    markdown = re.sub(r"[ \t]+", " ", render(root))
    markdown = re.sub(r"\n[ \t]+", "\n", markdown)
    return re.sub(r"\n{3,}", "\n\n", markdown).strip() + "\n"


def prepare():
    support = Support()
    manifest = {x["id"]: x for x in json.loads((ROOT / "fetch_manifest.json").read_text())}
    try:
        for query in json.loads((ROOT / "search_queries.json").read_text()):
            key = query["id"]
            search = parse_search((ROOT / "raw" / f"search-{key}.txt").read_text())
            write_json(ROOT / f"search-{key}.json", search)
            normalized = support.call({"op": "prepare", "adapter": "search-results", "input": search})
            write_json(ROOT / f"search-{key}-documents.json", normalized)
            row = manifest["jma-definition" if key == "jma" else key]
            if "error" in row:
                continue
            markdown = markdown_from_html((ROOT / "raw" / f"page-{row['id']}.html").read_bytes().decode(row["charset"]))
            (ROOT / f"page-{key}.md").write_text(markdown)
            page = {"id": key, "url": row["url"], "title": search["results"][0]["title"], "markdown": markdown}
            write_json(ROOT / f"page-{key}.json", page)
            normalized = support.call({"op": "prepare", "adapter": "markdown-page", "input": page})
            write_json(ROOT / f"page-{key}-documents.json", normalized)
            print(key, "search results", len(search["results"]), "page blocks", len(normalized["documents"][0]["blocks"]))
    finally:
        support.close()


class Support:
    def __init__(self):
        self.process = subprocess.Popen(
            [str(REPO / "target/debug/examples/web_selection_eval_support")],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True,
        )

    def call(self, value):
        self.process.stdin.write(compact(value) + "\n")
        self.process.stdin.flush()
        line = self.process.stdout.readline()
        if not line:
            raise RuntimeError("evaluation support exited")
        return json.loads(line)

    def count(self, text):
        return self.call({"op": "count", "text": text})["tokens"]

    def close(self):
        self.process.stdin.close()
        self.process.stdout.close()
        if self.process.wait(timeout=5):
            raise RuntimeError("evaluation support failed")


def write_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")


def validate_provenance(selected, originals):
    by_id = {d["id"]: d for d in originals}
    for doc in selected:
        source = by_id[doc["id"]]
        for key in ("url", "title", "source_hash"):
            if source[key] != doc[key]:
                raise AssertionError("source metadata changed")
        original = {b["id"]: b for b in source["blocks"]}
        returned = {b["id"] for b in doc["blocks"]}
        for block in doc["blocks"]:
            if original[block["id"]] != block:
                raise AssertionError("source block changed")
            dependencies = set(block.get("related_ids", []))
            if block.get("parent_id"):
                dependencies.add(block["parent_id"])
            if not dependencies <= returned:
                raise AssertionError("source context lost")


def run():
    config = json.loads((ROOT / "cases.json").read_text())
    frozen = json.loads((ROOT / "frozen_inputs.sha256.json").read_text())
    for name, expected in frozen.items():
        if hashlib.sha256((ROOT/name).read_bytes()).hexdigest() != expected:
            raise RuntimeError(f"frozen input changed: {name}")
    code_paths = ["src/selection.rs", "src/adapters.rs", "src/bin/jrg-select.rs", "target/debug/jrg-select"]
    identity = {p: hashlib.sha256((REPO/p).read_bytes()).hexdigest() for p in code_paths}
    support, rows, baselines = Support(), [], {}
    (ROOT / "outputs").mkdir(exist_ok=True)
    try:
        for case in config["cases"]:
            docs = json.loads((ROOT/case["documents_file"]).read_text())["documents"]
            source = json.loads((ROOT/case["input_file"]).read_text())
            raw_tokens = support.count(compact(source))
            normalized_tokens = support.count(compact({"documents": docs}))
            for unit in case["gold"]:
                matching = next(d for d in docs if d["url"] == unit["url"])
                blocks = {b["id"]: b for b in matching["blocks"]}
                if "source_text_sha256" in unit:
                    hashes = [hashlib.sha256(blocks[x]["text"].encode()).hexdigest() for x in unit["block_ids"]]
                    if hashes != unit["source_text_sha256"]:
                        raise AssertionError("gold source changed")
            for budget in config["budgets"]:
                cache_key = case["documents_file"], budget
                if cache_key not in baselines:
                    baselines[cache_key] = original_order(docs, budget, support.count)
                baseline = baselines[cache_key]
                request = {"query": case["query"], "max_tokens": budget, "input": source}
                elapsed, output = [], None
                for _ in range(3):
                    start = time.perf_counter_ns()
                    process = subprocess.run([str(REPO / "target/debug/jrg-select"), "--adapter", case["adapter"], "-"],
                                             input=compact(request), capture_output=True, text=True, timeout=20)
                    elapsed.append((time.perf_counter_ns()-start)/1e6)
                    if process.returncode:
                        raise RuntimeError(process.stderr)
                    current = json.loads(process.stdout)
                    if output is not None and current != output:
                        raise AssertionError("non-deterministic selection")
                    output = current
                for name, result in [("original_order", baseline), ("jrg", output)]:
                    actual = support.count(compact(result))
                    if actual != result["tokens"] or actual > budget:
                        raise AssertionError(f"invalid token budget {name}: {actual}, {result['tokens']}, {budget}")
                    validate_provenance(result["documents"], docs)
                    hits = evidence_score(result["documents"], case["gold"])
                    row = {k: case[k] for k in ("id", "topic", "stage", "language")}
                    row.update(method=name, budget=budget, evidence_hits=hits, evidence_total=len(case["gold"]),
                               recall=hits/len(case["gold"]), any_hit=hits>0, all_evidence=hits==len(case["gold"]),
                               raw_input_tokens=raw_tokens, normalized_input_tokens=normalized_tokens,
                               output_tokens=actual, reduction_vs_raw=1-actual/raw_tokens,
                               status=result["status"], documents=len(result["documents"]),
                               blocks=sum(len(d["blocks"]) for d in result["documents"]),
                               cli_ms=elapsed if name=="jrg" else None,
                               provenance_valid=True)
                    rows.append(row)
                    write_json(ROOT/"outputs"/f"{case['id']}-{budget}-{name}.json", result)
                print(case["id"], budget, "recall", f"{rows[-2]['recall']:.2f}->{rows[-1]['recall']:.2f}", flush=True)
    finally:
        support.close()
    for path, expected in identity.items():
        if hashlib.sha256((REPO/path).read_bytes()).hexdigest() != expected:
            raise AssertionError("selector changed during experiment")
    write_json(ROOT/"results.json", {"selector_identity": identity, "repetitions": 3, "rows": rows})
    summary = []
    for stage in ("search", "page"):
        for language in ("source_language", "japanese"):
            for budget in config["budgets"]:
                for method in ("original_order", "jrg"):
                    group = [r for r in rows if r["stage"]==stage and r["language"]==language and r["budget"]==budget and r["method"]==method]
                    if not group:
                        continue
                    hits, total = sum(r["evidence_hits"] for r in group), sum(r["evidence_total"] for r in group)
                    summary.append({"stage":stage,"language":language,"budget":budget,"method":method,"cases":len(group),
                                    "hits":hits,"total":total,"micro_recall":hits/total,
                                    "all_evidence_cases":sum(r["all_evidence"] for r in group),
                                    "any_hit_cases":sum(r["any_hit"] for r in group),
                                    "output_tokens":sum(r["output_tokens"] for r in group),
                                    "raw_input_tokens":sum(r["raw_input_tokens"] for r in group),
                                    "reduction_vs_raw":1-sum(r["output_tokens"] for r in group)/sum(r["raw_input_tokens"] for r in group),
                                    "median_cli_ms":statistics.median([x for r in group for x in r["cli_ms"]]) if method=="jrg" else None})
    write_json(ROOT/"summary.json", summary)
    print("Completed", len(rows), "method rows", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=["prepare", "run"])
    args = parser.parse_args()
    prepare() if args.command == "prepare" else run()
