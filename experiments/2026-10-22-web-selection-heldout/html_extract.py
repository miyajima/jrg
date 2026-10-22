"""Reuse the prior renderer; handle HTML's optional element end tags."""
from pathlib import Path
import sys

sys.path.insert(0,str(Path(__file__).resolve().parent.parent/"2026-10-22-web-selection-eval"))
import evaluate as previous

class OptionalEndParser(previous.PageParser):
    BLOCK_STARTS = {"p","div","section","article","aside","blockquote","pre","ul","ol","dl","table","h1","h2","h3","h4","h5","h6","hr"}

    def close_after_boundary(self, targets, boundaries):
        boundary = max((i for i,node in enumerate(self.stack) if node.tag in boundaries),default=0)
        for i in range(len(self.stack)-1,boundary,-1):
            if self.stack[i].tag in targets:
                del self.stack[i:]
                return

    def handle_starttag(self, tag, attrs):
        if tag in self.BLOCK_STARTS:
            self.close_after_boundary({"p"},{"body","article","main","section","div","li","td","th"})
        if tag=="li":
            self.close_after_boundary({"li"},{"ul","ol"})
        elif tag in {"dt","dd"}:
            self.close_after_boundary({"dt","dd"},{"dl"})
        elif tag=="tr":
            self.close_after_boundary({"tr"},{"table"})
        elif tag in {"td","th"}:
            self.close_after_boundary({"td","th"},{"tr"})
        elif tag=="option":
            self.close_after_boundary({"option"},{"select","optgroup"})
        super().handle_starttag(tag,attrs)


# Only this process changes the parser used by the shared renderer. No file or
# product adapter is modified; both evaluation arms use this same conversion.
previous.PageParser = OptionalEndParser
markdown_from_html = previous.markdown_from_html
title_from_html = previous.title_from_html
