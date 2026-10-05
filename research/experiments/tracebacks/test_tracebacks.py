import linecache
import traceback

import pytest
from tracebacks import RATIO_SOURCE, load_module, ratio_tree


def failure(module) -> traceback.TracebackException:
    with pytest.raises(ZeroDivisionError) as caught:
        module.ratio(None)
    return traceback.TracebackException.from_exception(caught.value)


def lotml_frame(module):
    return failure(module).stack[-1]


def test_frame_names_the_lotml_file_and_line(tmp_path):
    path = tmp_path / "ratio.lotml"
    path.write_text(RATIO_SOURCE, encoding="utf-8")
    frame = lotml_frame(load_module(ratio_tree(), path))
    assert frame.filename == str(path)
    assert frame.lineno == 2
    assert frame.name == "ratio"


def test_shown_line_is_the_lotml_source_not_the_python(tmp_path):
    path = tmp_path / "ratio.lotml"
    path.write_text(RATIO_SOURCE, encoding="utf-8")
    assert lotml_frame(load_module(ratio_tree(), path)).line == "return 10 // (d ?? 0)"


def test_column_span_is_the_lotml_expression(tmp_path):
    path = tmp_path / "ratio.lotml"
    path.write_text(RATIO_SOURCE, encoding="utf-8")
    frame = lotml_frame(load_module(ratio_tree(), path))
    line = RATIO_SOURCE.splitlines()[1]
    assert (frame.colno, frame.end_colno) == (11, len(line))
    assert line[frame.colno : frame.end_colno] == "10 // (d ?? 0)"


def test_caret_line_underlines_the_lotml_expression(tmp_path):
    path = tmp_path / "ratio.lotml"
    path.write_text(RATIO_SOURCE, encoding="utf-8")
    text = "".join(failure(load_module(ratio_tree(), path)).format())
    shown = "    return 10 // (d ?? 0)"
    lines = text.splitlines()
    marks = lines[lines.index(shown) + 1]
    assert marks.strip() and set(marks.strip()) <= {"~", "^"}
    assert shown[len(marks) - len(marks.lstrip()) :].startswith("10 // (d ?? 0)")


def test_source_registered_in_linecache_needs_no_file_on_disk(tmp_path):
    path = tmp_path / "never-written.lotml"
    frame = lotml_frame(load_module(ratio_tree(), path, source=RATIO_SOURCE))
    assert frame.line == "return 10 // (d ?? 0)"
    linecache.checkcache(str(path))
