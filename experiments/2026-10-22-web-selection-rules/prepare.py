#!/usr/bin/env python3
"""Prepare frozen official HTML without inspecting or executing selection."""
import hashlib
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parent
PREVIOUS = ROOT.parent/"2026-10-22-web-selection-eval"
sys.path.insert(0,str(PREVIOUS))
from evaluate import Support, write_json
from html_extract import markdown_from_html, title_from_html


def main():
    questions = json.loads((ROOT/"questions.json").read_text())["questions"]
    manifest = {r["id"]:r for r in json.loads((ROOT/"fetch_manifest.json").read_text())}
    support, prepared = Support(), []
    try:
        for question in questions:
            row = manifest[question["id"]]
            if "error" in row:
                prepared.append({"id":question["id"],"excluded":row["error"]})
                continue
            content = (ROOT/"raw"/f"{question['id']}.html").read_bytes()
            assert hashlib.sha256(content).hexdigest()==row["sha256"]
            html = content.decode(row["charset"])
            markdown = markdown_from_html(html)
            page = {"id":question["id"],"url":row["final_url"],"title":title_from_html(html),"markdown":markdown}
            write_json(ROOT/f"{question['id']}-page.json",page)
            (ROOT/f"{question['id']}.md").write_text(markdown)
            documents = support.call({"op":"prepare","adapter":"markdown-page","input":page})
            write_json(ROOT/f"{question['id']}-documents.json",documents)
            blocks = documents["documents"][0]["blocks"]
            prepared.append({"id":question["id"],"title":page["title"],"blocks":len(blocks),
                             "input_tokens":support.count(json.dumps(page,ensure_ascii=False,separators=(",",":")))})
    finally:
        support.close()
    write_json(ROOT/"prepared_manifest.json",{"optional_end_tag_fix_sha256":hashlib.sha256((ROOT/"html_extract.py").read_bytes()).hexdigest(),"preprocessor":str(PREVIOUS/"evaluate.py"),
               "preprocessor_sha256":hashlib.sha256((PREVIOUS/"evaluate.py").read_bytes()).hexdigest(),"documents":prepared})
    print(json.dumps(prepared,ensure_ascii=False,indent=2))


if __name__=="__main__":
    main()
