import json

import pytest
from verify import MIN_QUOTE, canonical, find_quote, load_claims


def test_ligatures_and_case_do_not_matter():
    assert canonical("The ﬁrst Field") == canonical("the first field")


def test_hyphen_split_across_lines_matches_the_whole_word():
    assert canonical("type-con-\nstrained decod-\ning") == canonical(
        "type-constrained decoding"
    )


def test_typographic_quotes_and_minus_signs_are_folded():
    assert canonical("“don’t” −5") == canonical('"don\'t" -5')


def test_quote_is_found_on_its_page():
    pages = ["intro text", "results: on average 94% of compilation\nerrors were types"]
    assert find_quote(pages, "on average 94% of compilation errors") == 2


def test_missing_quote_returns_none():
    assert find_quote(["on average 94% of errors"], "on average 49% of errors") is None


def test_digits_and_decimal_points_are_kept():
    pages = ["pass@1 rose to 10.4%"]
    assert find_quote(pages, "pass@1 rose to 104%") is None
    assert find_quote(pages, "pass@1 rose to 10.4%") == 1


def test_claims_must_cite_a_known_source_and_a_quote_long_enough(tmp_path):
    sources = tmp_path / "sources.json"
    sources.write_text(json.dumps([{"id": "s1", "url": "https://x/s1.pdf"}]))
    claims = tmp_path / "claims.json"
    claims.write_text(json.dumps([{"source": "s2", "quote": "x" * MIN_QUOTE}]))
    with pytest.raises(ValueError, match="unknown source"):
        load_claims(claims, sources)
    claims.write_text(json.dumps([{"source": "s1", "quote": "too short"}]))
    with pytest.raises(ValueError, match="shorter than"):
        load_claims(claims, sources)


def test_sources_need_a_plain_file_name_and_https():
    from verify import check_source

    check_source({"id": "2504.09246", "url": "https://arxiv.org/pdf/2504.09246v2"})
    with pytest.raises(ValueError, match="file name"):
        check_source({"id": "../escape", "url": "https://x/y.pdf"})
    with pytest.raises(ValueError, match="https"):
        check_source({"id": "s1", "url": "file:///etc/passwd"})


def test_signs_and_comparisons_next_to_numbers_are_kept():
    assert canonical("p < 0.05") != canonical("p > 0.05")
    assert canonical("fell by -5%") != canonical("fell by 5%")
    assert canonical("10-20 points") != canonical("1020 points")
    assert canonical("+3 tokens") != canonical("3 tokens")


def test_hyphens_between_words_still_fold():
    assert canonical("type-constrained") == canonical("type constrained")
