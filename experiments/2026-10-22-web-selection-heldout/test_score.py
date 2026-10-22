import unittest

from score import score_units


class EvidenceScoreTests(unittest.TestCase):
    def test_alternatives_accept_either_but_require_entire_group(self):
        units = [{"id": "token", "url": "https://source/", "alternatives": [["a", "b"], ["c"]]}]
        def selected(*ids):
            return [{"url": "https://source/", "blocks": [{"id": x} for x in ids]}]
        self.assertEqual(score_units(selected("a"), units), [])
        self.assertEqual(score_units(selected("a", "b"), units), ["token"])
        self.assertEqual(score_units(selected("c"), units), ["token"])
        self.assertEqual(score_units(selected("a", "b", "c"), units), ["token"])

    def test_blocks_from_wrong_source_or_separate_documents_do_not_combine(self):
        units = [{"id": "fact", "url": "https://source/", "alternatives": [["a", "b"]]}]
        self.assertEqual(score_units([{"url": "https://other/", "blocks": [{"id": "a"}, {"id": "b"}]}], units), [])
        self.assertEqual(score_units([
            {"url": "https://source/", "blocks": [{"id": "a"}]},
            {"url": "https://source/", "blocks": [{"id": "b"}]},
        ], units), [])


if __name__ == "__main__":
    unittest.main()
