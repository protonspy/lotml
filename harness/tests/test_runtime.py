import pytest

from lotml_harness.lang import runtime


def test_unwrap_returns_a_value_or_fails_with_the_error():
    assert runtime.unwrap(runtime.Ok(3)) == 3
    with pytest.raises(runtime.Fail) as caught:
        runtime.unwrap(runtime.Err("e"))
    assert caught.value.error == "e"
    with pytest.raises(runtime.LotmlTypeError):
        runtime.unwrap(3)


def test_coalesce_evaluates_the_default_only_when_absent():
    assert runtime.coalesce(0, lambda: runtime.fail("never")) == 0
    assert runtime.coalesce(None, lambda: 5) == 5


def test_i64_traps_outside_the_range_and_ignores_other_values():
    assert runtime.i64(runtime.I64_MAX) == runtime.I64_MAX
    assert runtime.i64(2.0**70) == 2.0**70
    with pytest.raises(runtime.Overflow):
        runtime.i64(runtime.I64_MAX + 1)
    with pytest.raises(runtime.Overflow):
        runtime.i64(runtime.I64_MIN - 1)


def test_wrapping_arithmetic_is_modular():
    assert runtime.wrapping_add(runtime.I64_MAX, 1) == runtime.I64_MIN
    assert runtime.wrapping_sub(runtime.I64_MIN, 1) == runtime.I64_MAX
    assert runtime.wrapping_mul(2**62, 4) == 0


def test_conditions_accept_only_bool_in_variant_b():
    assert runtime.condition(True) is True
    with pytest.raises(runtime.LotmlTypeError):
        runtime.condition([1])


def test_variant_a_tests_optionals_for_presence():
    assert runtime.truth_a(0) is True
    assert runtime.truth_a(None) is False
    assert runtime.or_a(None, lambda: 7) == 7
    assert runtime.or_a(0, lambda: 7) == 0
    assert runtime.or_a(False, lambda: True) is True


def test_value_copies_mutable_values_and_shares_the_rest():
    items = [[1]]
    copy = runtime.value(items)
    copy[0].append(2)
    assert items == [[1]]
    unit = runtime.Unit("Empty")
    box = runtime.Box([1])
    assert runtime.value(unit) is unit
    assert runtime.value(box) is box
    assert runtime.value(len) is len


def test_records_compare_hash_and_take_fresh_defaults():
    point = runtime.record("Point", ("x", "tags"), {"tags": list})
    a, b = point(1), point(1)
    a.tags.append("t")
    assert b.tags == []
    assert point(1, [2]) == point(1, [2])
    assert hash(point(1, [2])) == hash(point(1, [2]))
    assert point[int] is point
    assert sorted([point(2), point(1)]) == [point(1), point(2)]


def test_units_are_singletons_that_sort_by_name():
    a, b = runtime.Unit("A"), runtime.Unit("B")
    assert repr(a) == "A"
    assert sorted([b, a]) == [a, b]


def test_trait_defaults_fill_only_missing_methods():
    class T:
        def kept(self):
            return "own"

    runtime.attach_defaults(T, {"kept": lambda self: "default", "added": lambda self: "d"})
    assert T().kept() == "own"
    assert T().added() == "d"


def test_heap_pops_the_minimum_or_none():
    heap = runtime.Heap([3, 1, 2])
    heap.push(0)
    assert heap.peek() == 0
    assert [heap.pop_min() for _ in range(5)] == [0, 1, 2, 3, None]
    assert heap.peek() is None
    assert len(heap) == 0
    assert runtime.Heap([2, 1]) == runtime.Heap([1, 2])


@pytest.mark.parametrize(
    ("obj", "name", "args", "expected"),
    [
        ("42", "to_int", (), 42),
        ("x", "to_int", (), None),
        ("1.5", "to_float", (), 1.5),
        ("abc", "find", ("c",), 2),
        ("abc", "find", ("z",), None),
        ("a=b", "split_once", ("=",), ("a", "b")),
        ("ab", "split_once", ("=",), None),
        ([], "pop", (), None),
        ([1, 2], "pop", (), 2),
        ([], "last", (), None),
        ([1, 2], "find", (lambda x: x > 1,), 2),
        ([1, 2], "contains", (2,), True),
        ([1, 2], "index", (3,), None),
        ({"a": 1}, "pop", ("b",), None),
        ({"a": 1}, "pop", ("b", 0), 0),
        ({"a": 1}, "keys", (), ["a"]),
        ({"a": 1}, "items", (), [("a", 1)]),
        ("ab", "upper", (), "AB"),
    ],
)
def test_methods_whose_lotml_result_differs_from_python(obj, name, args, expected):
    assert runtime.method(obj, name, *args) == expected


def test_todo_and_no_match_stop_the_program():
    with pytest.raises(runtime.Todo):
        runtime.todo()
    with pytest.raises(runtime.NonExhaustiveMatch):
        runtime.no_match(1)


def test_the_prelude_returns_lists_where_python_returns_iterators():
    prelude = runtime.PRELUDE
    assert prelude["reversed"]([1, 2]) == [2, 1]
    assert prelude["zip"]([1], [2]) == [(1, 2)]
    assert prelude["map"](abs, [-1]) == [1]
    assert prelude["filter"](None, [0, 1]) == [1]
    assert "open" not in prelude and "eval" not in prelude


def test_writeback_helpers_store_into_fields_and_elements():
    class Holder:
        items = None

    holder, items = Holder(), [0]
    runtime.set_attr(holder, "items", [1])
    runtime.set_item(items, 0, 5)
    assert holder.items == [1]
    assert items == [5]
    assert runtime.returning(3, None, None) == 3
