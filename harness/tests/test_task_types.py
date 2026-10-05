import ast

import pytest

from lotml_harness.tasks.types import (
    Dict,
    List,
    Optional,
    Prim,
    Set,
    Tuple,
    Unit,
    UnsupportedType,
    from_annotation,
    parse,
)


def annotation(text: str) -> ast.expr:
    return ast.parse(text, mode="eval").body


@pytest.mark.parametrize(
    ("python", "lotml"),
    [
        ("int", "int"),
        ("float", "f64"),
        ("str", "str"),
        ("bool", "bool"),
        ("None", "None"),
        ("List[int]", "[int]"),
        ("list[str]", "[str]"),
        ("Dict[str, List[float]]", "{str: [f64]}"),
        ("dict[int, int]", "{int: int}"),
        ("Tuple[int, str]", "(int, str)"),
        ("tuple[int, int, bool]", "(int, int, bool)"),
        ("Set[int]", "{int}"),
        ("set[str]", "{str}"),
        ("Optional[int]", "int?"),
        ("Optional[List[int]]", "[int]?"),
        ("int | None", "int?"),
        ("List[Optional[str]]", "[str?]"),
    ],
)
def test_python_annotations_become_lotml_types(python, lotml):
    assert from_annotation(annotation(python)).render("b") == lotml


@pytest.mark.parametrize(
    "python",
    [
        "Any",
        "Union[int, str]",
        "int | str",
        "Callable[[int], int]",
        "List",
        "Tuple[int, ...]",
    ],
)
def test_types_lotml_cannot_express_are_refused(python):
    with pytest.raises(UnsupportedType):
        from_annotation(annotation(python))


def test_variant_a_spells_the_unit_type_none_in_lowercase():
    assert Unit().render("a") == "none"
    assert Optional(Unit()).render("b") == "None?"


@pytest.mark.parametrize(
    "text",
    [
        "int",
        "f64",
        "[int]",
        "{str: [f64]}",
        "(int, str)",
        "{int}",
        "int?",
        "[str?]",
        "None",
    ],
)
def test_parse_reads_back_what_render_writes(text):
    assert parse(text).render("b") == text


def test_parse_builds_the_structure():
    assert parse("{str: [(int, f64?)]}") == Dict(
        Prim("str"), List(Tuple((Prim("int"), Optional(Prim("f64")))))
    )
    assert parse("{bool}") == Set(Prim("bool"))


@pytest.mark.parametrize("text", ["[int", "int int", "{int: }", "Foo"])
def test_parse_rejects_garbage(text):
    with pytest.raises(UnsupportedType):
        parse(text)
