#!/usr/bin/env python3
"""Verify the implemented expansion field against the existing frozen probe."""
import hashlib
import json
import subprocess
from evaluate import ROOT, REPO, Support, compact, evidence_score, validate_provenance, write_json


def main():
    cases = json.loads((ROOT/"cases.json").read_text())["cases"]
    support, rows = Support(), []
    try:
        for topic in ("rust", "python", "node", "github"):
            jp = next(c for c in cases if c["id"] == f"page-{topic}-japanese")
            en = next(c for c in cases if c["id"] == f"page-{topic}-source_language")
            source = json.loads((ROOT/jp["input_file"]).read_text())
            docs = json.loads((ROOT/jp["documents_file"]).read_text())["documents"]

            def select(query, **extra):
                result = subprocess.run([str(REPO/"target/debug/jrg-select"), "--adapter", jp["adapter"], "-"],
                                        input=compact({"query":query,"max_tokens":1024,"input":source,**extra}),
                                        text=True,capture_output=True,timeout=20,check=True)
                value = json.loads(result.stdout)
                validate_provenance(value["documents"], docs)
                assert support.count(compact(value)) == value["tokens"] <= 1024
                return value

            primary = select(jp["query"])
            english = select(en["query"])
            for case, result in ((jp, primary), (en, english)):
                assert result == json.loads((ROOT/"outputs"/f"{case['id']}-1024-jrg.json").read_text())
            combined = select(jp["query"]+"\n"+en["query"])
            expanded = select(jp["query"], query_expansions=[en["query"]])
            assert expanded == combined
            rows.append({"topic":topic,"budget":1024,"legacy_outputs_unchanged":True,
                         "matches_concatenated_probe":True,"japanese_hits":evidence_score(primary["documents"],jp["gold"]),
                         "expanded_hits":evidence_score(expanded["documents"],jp["gold"]),"total":len(jp["gold"]),
                         "tokens":expanded["tokens"],"query":jp["query"],"query_expansions":[en["query"]],
                         "selected":expanded})
            print({k:v for k,v in rows[-1].items() if k not in ("selected","query","query_expansions")},flush=True)
    finally:
        support.close()
    identity = {p:hashlib.sha256((REPO/p).read_bytes()).hexdigest() for p in
                ("src/selection.rs","src/bin/jrg-select.rs","target/debug/jrg-select")}
    write_json(ROOT/"bilingual_implementation_results.json", {"selector_identity":identity,"rows":rows,
               "limitation":"Known four pages and pre-existing manual English questions; no automatic translation or held-out quality test."})


if __name__ == "__main__":
    main()
