import unittest
from html_extract import markdown_from_html, title_from_html


class HtmlExtractionTests(unittest.TestCase):
    def test_optional_paragraph_end_tags_do_not_create_recursive_nesting(self):
        html = "<html><head><title>Source</title></head><body>" + "".join(f"<p>Evidence {i}." for i in range(1100)) + "</body></html>"
        markdown = markdown_from_html(html)
        self.assertIn("Evidence 0.",markdown)
        self.assertIn("Evidence 1099.",markdown)
        self.assertEqual(markdown.count("Evidence "),1100)

    def test_optional_cells_and_nested_lists_keep_all_source_conditions(self):
        html = "<html><head><title>A &amp; B</title></head><body><table><tr><th>Type<th>Rule<tr><td>A<td>Except B</table><ul><li>Outer<ul><li>Inner<li>Exception</ul><li>Other</ul></body></html>"
        markdown = markdown_from_html(html)
        self.assertEqual(title_from_html(html),"A & B")
        self.assertIn("| Type | Rule |",markdown)
        self.assertIn("| A | Except B |",markdown)
        for text in ("Outer","Inner","Exception","Other"):
            self.assertIn(text,markdown)


if __name__=="__main__":
    unittest.main()
