"""The GitHub Pages site under `site/`: every LotML program it shows checks and passes its tests,
and every link it makes inside the site lands somewhere (plans/github-pages-site.md)."""

from html.parser import HTMLParser
from pathlib import Path

import pytest

from lotml_harness import ROOT
from lotml_harness.experiments.phase1 import Lotml

SITE = ROOT / "site"
PAGE = SITE / "index.html"


class Page(HTMLParser):
    """The text of each `<code class="lang-lot">`, the ids, and the hrefs and srcs of a page."""

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.snippets: list[str] = []
        self.ids: set[str] = set()
        self.targets: list[str] = []
        self.depth = 0
        """How many elements deep inside a LotML snippet the parser is; 0 when outside one."""

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if "id" in attrs:
            self.ids.add(attrs["id"])
        for name in ("href", "src"):
            if attrs.get(name):
                self.targets.append(attrs[name])
        if self.depth:
            self.depth += 1
        elif tag == "code" and "lang-lot" in (attrs.get("class") or "").split():
            self.depth = 1
            self.snippets.append("")

    def handle_endtag(self, tag):
        if self.depth:
            self.depth -= 1

    def handle_data(self, data):
        if self.depth:
            self.snippets[-1] += data


def parsed() -> Page:
    page = Page()
    page.feed(PAGE.read_text(encoding="utf-8"))
    return page


SNIPPETS = parsed().snippets


def test_the_page_shows_lotml_programs():
    assert len(SNIPPETS) >= 5


@pytest.mark.parametrize("snippet", SNIPPETS, ids=lambda s: s.split("\n", 1)[0][:40])
def test_each_lotml_snippet_checks_and_its_tests_pass(snippet, tmp_path: Path):
    (tmp_path / "snippet.lot").write_text(snippet + "\n", encoding="utf-8")
    lotml = Lotml()
    check = lotml.compiler(["check", "snippet.lot"], str(tmp_path))
    assert check is not None and check.returncode == 0, check and check.stdout + check.stderr
    tested = lotml.compiler(["test", "snippet.lot"], str(tmp_path))
    assert tested is not None and tested.returncode == 0, tested and tested.stdout + tested.stderr


def test_every_fragment_link_names_an_id_on_the_page():
    page = parsed()
    fragments = [t[1:] for t in page.targets if t.startswith("#")]
    assert fragments
    assert [f for f in fragments if f not in page.ids] == []


def test_every_local_file_the_page_links_exists():
    local = [
        t.split("#", 1)[0]
        for t in parsed().targets
        if not t.startswith(("#", "http://", "https://", "mailto:"))
    ]
    assert local, "the stylesheet, the script and the icon at least"
    assert [t for t in local if not (SITE / t).is_file()] == []
