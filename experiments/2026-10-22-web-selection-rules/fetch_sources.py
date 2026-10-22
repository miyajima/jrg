#!/usr/bin/env python3
"""Read only the eight predeclared public documentation pages."""
import datetime
import hashlib
import json
from pathlib import Path
import urllib.request
from concurrent.futures import ThreadPoolExecutor

root = Path(__file__).resolve().parent
questions = json.loads((root/"questions.json").read_text())["questions"]


def fetch(row):
    result = {"id":row["id"],"url":row["url"],"retrieved_at":datetime.datetime.now(datetime.timezone.utc).isoformat()}
    try:
        request = urllib.request.Request(row["url"],headers={"User-Agent":"jrg-selection-evaluation/0.1"})
        with urllib.request.urlopen(request,timeout=30) as response:
            content = response.read(4*1024*1024+1)
            result.update(final_url=response.url,charset=response.headers.get_content_charset() or "utf-8")
        if len(content)>4*1024*1024:
            raise ValueError("HTML exceeds 4 MiB")
        (root/"raw"/f"{row['id']}.html").write_bytes(content)
        result.update(bytes=len(content),sha256=hashlib.sha256(content).hexdigest())
    except Exception as error:
        result["error"] = str(error)
    return result


if __name__ == "__main__":
    manifest = root/"fetch_manifest.json"
    if manifest.exists() and not (root/"fetch_sandbox_attempt.json").exists():
        manifest.rename(root/"fetch_sandbox_attempt.json")
    rows = list(ThreadPoolExecutor(max_workers=4).map(fetch,questions))
    manifest.write_text(json.dumps(rows,ensure_ascii=False,indent=2)+"\n")
    print(json.dumps([{k:v for k,v in row.items() if k not in ("sha256","url","final_url","retrieved_at")}
                      for row in rows],ensure_ascii=False,indent=2))
