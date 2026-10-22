import unittest
from evaluate import evidence_score, original_order, parse_search, compact


class EvaluationTests(unittest.TestCase):
    def test_search_parser_keeps_provider_order_and_returned_text(self):
        raw = "First (https://one.test/)\nciteturn0search0 [wordlim: 200]\n\nOriginal A.\nSecond (https://two.test/)\nciteturn0search1 [wordlim: 200]\n\nOriginal B.\n"
        found = parse_search(raw)["results"]
        self.assertEqual([x["url"] for x in found], ["https://one.test/", "https://two.test/"])
        self.assertIn("Original A.", found[0]["text"])
        self.assertNotIn("Original B.", found[0]["text"])

    def test_evidence_units_require_all_blocks_and_correct_source(self):
        doc = {"url": "https://one.test/", "blocks": [{"id": "p", "text": "value"}]}
        gold = [{"url": doc["url"], "block_ids": ["p", "n"]}]
        self.assertEqual(evidence_score([doc], gold), 0)
        doc["blocks"].append({"id": "n", "text": "exception"})
        self.assertEqual(evidence_score([doc], gold), 1)
        doc["url"] = "https://other.test/"
        self.assertEqual(evidence_score([doc], gold), 0)

    def test_link_only_result_uses_original_title_without_dropping_candidate(self):
        raw = "Title (https://one.test/)\nciteturn0search0 [wordlim: 200]\n"
        found = parse_search(raw)["results"]
        self.assertEqual(len(found), 1)
        self.assertEqual(found[0]["text"], "Title")

    def test_budget_baseline_keeps_context_or_skips_whole_group(self):
        doc = {"id": "d", "url": "https://one.test/", "title": "Title", "source_hash": "h", "blocks": [
            {"id": "heading", "kind": "heading", "text": "Heading"},
            {"id": "p", "kind": "paragraph", "text": "a" * 400, "parent_id": "heading", "related_ids": ["n"]},
            {"id": "n", "kind": "note", "text": "note", "parent_id": "heading"},
            {"id": "small", "kind": "paragraph", "text": "small"},
        ]}
        out = original_order([doc], 450, len)
        self.assertLessEqual(len(compact(out)), 450)
        ids = {b["id"] for d in out["documents"] for b in d["blocks"]}
        self.assertNotIn("p", ids)
        self.assertIn("small", ids)
        if "n" in ids:
            self.assertIn("heading", ids)


if __name__ == "__main__":
    unittest.main()
