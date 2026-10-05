"""The editing pilot's tasks: one change per paired-corpus program, each with a hidden test.

Every task changes block structure — a new branch, a guard, a nested condition, a new
function — so the edit has to put lines at the right depth. `reference` is one correct
edit in the indented form, used only to check that the hidden test tells a correct edit
from the unedited program.
"""

from dataclasses import dataclass


def lines(*text: str) -> str:
    return "".join(line + "\n" for line in text)


@dataclass(frozen=True)
class Task:
    program: str
    instruction: str
    test: str
    reference: tuple[tuple[str, str], ...]


TASKS = [
    Task(
        "01-find-adult",
        (
            "Add a variant `NoEmail(name: str)` to `LookupErr`. `find_adult` must fail with"
            " `NoEmail(name)` when the adult user it found has no email."
        ),
        lines(
            'test "adult without email":',
            '    users = [User("bo", 30), User("cy", 40, "cy@x.org")]',
            '    assert find_adult(users, "bo") == Err(NoEmail("bo"))',
            '    assert find_adult(users, "cy")? == User("cy", 40, "cy@x.org")',
        ),
        (
            (
                lines("type LookupErr = NotFound(name: str) | Minor(age: int)"),
                lines(
                    "type LookupErr = NotFound(name: str) | Minor(age: int) | NoEmail(name: str)"
                ),
            ),
            (
                lines("        fail Minor(u.age)"),
                lines(
                    "        fail Minor(u.age)",
                    "    if u.email is None:",
                    "        fail NoEmail(name)",
                ),
            ),
        ),
    ),
    Task(
        "02-shape-area",
        "Add a shape `Square(side: f64)` to `Shape` and make `area` handle it.",
        lines(
            'test "square":',
            "    assert area(Square(3.0)) == 9.0",
            "    assert total_area([Square(2.0), Rect(1.0, 1.0)]) == 5.0",
        ),
        (
            (
                lines(
                    "type Shape = Circle(r: f64) | Rect(w: f64, h: f64) | Triangle(a: f64, b: f64, c: f64)"
                ),
                lines(
                    "type Shape = Circle(r: f64) | Rect(w: f64, h: f64) | Triangle(a: f64, b: f64, c: f64)"
                    " | Square(side: f64)"
                ),
            ),
            (
                lines("            return sqrt(p * (p - a) * (p - b) * (p - c))"),
                lines(
                    "            return sqrt(p * (p - a) * (p - b) * (p - c))",
                    "        case Square(side):",
                    "            return side * side",
                ),
            ),
        ),
    ),
    Task(
        "03-top-words",
        "`top_words` must ignore words shorter than 3 letters.",
        lines(
            'test "short words":',
            '    assert top_words("a a a the cat the", 1) == [("the", 2)]',
        ),
        (
            (
                lines("        if w.isalpha():"),
                lines("        if w.isalpha() and len(w) >= 3:"),
            ),
        ),
    ),
    Task(
        "04-binary-search",
        (
            "When `target` occurs more than once, `binary_search` must return the index of its"
            " first occurrence."
        ),
        lines(
            'test "first occurrence":',
            "    assert binary_search([1, 2, 2, 2, 3], 2) == 1",
            "    assert binary_search([2, 2], 2) == 0",
        ),
        (
            (
                lines("    var hi = len(xs) - 1"),
                lines("    var hi = len(xs) - 1", "    var found: int? = None"),
            ),
            (
                lines("            return mid", "        if xs[mid] < target:"),
                lines(
                    "            found = mid",
                    "            hi = mid - 1",
                    "        elif xs[mid] < target:",
                ),
            ),
            (lines("    return None"), lines("    return found")),
        ),
    ),
    Task(
        "05-parse-config",
        (
            '`parse_config` must fail with `ParseError(number, "duplicate key")` when a key'
            " appears a second time."
        ),
        lines(
            'test "duplicate key":',
            '    assert parse_config("a=1\\nb=2\\na=3") == Err(ParseError(3, "duplicate key"))',
        ),
        (
            (
                lines("        key, value = parse_line(trimmed, number)?"),
                lines(
                    "        key, value = parse_line(trimmed, number)?",
                    "        if key in config:",
                    '            fail ParseError(number, "duplicate key")',
                ),
            ),
        ),
    ),
    Task(
        "06-bank-transfer",
        (
            "Add a variant `SameAccount(account: str)` to `BankErr`. `transfer` must fail with"
            " `SameAccount(source)`, changing nothing, when `source` and `target` are the same"
            " account."
        ),
        lines(
            'test "same account":',
            '    var bank = Bank({"a": 100})',
            '    assert bank.transfer("a", "a", 10) == Err(SameAccount("a"))',
            '    assert bank.balances == {"a": 100}',
        ),
        (
            (
                lines(
                    "type BankErr = UnknownAccount(account: str) | InsufficientFunds(account: str)"
                ),
                lines(
                    "type BankErr = UnknownAccount(account: str) | InsufficientFunds(account: str)"
                    " | SameAccount(account: str)"
                ),
            ),
            (
                lines("        if target not in self.balances:"),
                lines(
                    "        if source == target:",
                    "            fail SameAccount(source)",
                    "        if target not in self.balances:",
                ),
            ),
        ),
    ),
    Task(
        "07-stack",
        (
            "Add a method `pop_n(var self, n: int) -> [T]` to `Stack` that pops up to `n` items"
            " and returns them, most recent first; it stops early when the stack is empty."
        ),
        lines(
            'test "pop n":',
            "    var s = Stack[int]()",
            "    s.push(1)",
            "    s.push(2)",
            "    s.push(3)",
            "    assert s.pop_n(2) == [3, 2]",
            "    assert s.pop_n(5) == [1]",
            "    assert s.size() == 0",
        ),
        (
            (
                lines("        return len(self.items)"),
                lines(
                    "        return len(self.items)",
                    "",
                    "    fn pop_n(var self, n: int) -> [T]:",
                    "        var out: [T] = []",
                    "        for _ in range(n):",
                    "            item = self.pop()",
                    "            if item is None:",
                    "                break",
                    "            out.append(item)",
                    "        return out",
                ),
            ),
        ),
    ),
    Task(
        "08-merge-intervals",
        "`merge` must skip any interval whose `start` is greater than its `end`.",
        lines(
            'test "inverted intervals":',
            "    assert merge([Interval(5, 1), Interval(1, 2), Interval(2, 3)]) == [Interval(1, 3)]",
        ),
        (
            (
                lines("    for iv in sorted(intervals, key=lambda iv: iv.start):"),
                lines(
                    "    for iv in sorted(intervals, key=lambda iv: iv.start):",
                    "        if iv.start > iv.end:",
                    "            continue",
                ),
            ),
        ),
    ),
    Task(
        "09-json-tree",
        (
            "Add a function `count_numbers(j: Json) -> int` that returns how many `Num` values"
            " the tree contains, at any depth."
        ),
        lines(
            'test "count numbers":',
            '    doc = Obj({"a": Arr([Num(1.0), Null, Num(2.0)]), "b": Num(3.0)})',
            "    assert count_numbers(doc) == 3",
            "    assert count_numbers(Null) == 0",
        ),
        (
            (
                lines('test "render and depth":'),
                lines(
                    "fn count_numbers(j: Json) -> int:",
                    "    match j:",
                    "        case Num(value):",
                    "            return 1",
                    "        case Arr(items):",
                    "            return sum(count_numbers(i) for i in items)",
                    "        case Obj(fields):",
                    "            return sum(count_numbers(v) for v in fields.values())",
                    "        case _:",
                    "            return 0",
                    "",
                    'test "render and depth":',
                ),
            ),
        ),
    ),
    Task(
        "10-fetch-retry",
        (
            "Add a field `calls: int = 0` to `Flaky` and make its `fetch` increment it on every"
            " call, whether the call fails or succeeds."
        ),
        lines(
            'test "calls":',
            "    var f = Flaky(1)",
            '    first = f.fetch("x")',
            '    second = f.fetch("x")',
            "    assert f.calls == 2",
            '    assert second? == "ok"',
        ),
        (
            (
                lines("type Flaky(failures: int)"),
                lines("type Flaky(failures: int, calls: int = 0)"),
            ),
            (
                lines("        if self.failures > 0:"),
                lines("        self.calls += 1", "        if self.failures > 0:"),
            ),
        ),
    ),
    Task(
        "11-priced-report",
        'A `Book` whose title starts with "Free" has price 0.0.',
        lines(
            'test "free books":',
            '    assert Book("Free Guide", 5.0).price() == 0.0',
            '    assert Bundle("X", [Book("Free A", 4.0), Book("B", 6.0)], 0.0).price() == 6.0',
        ),
        (
            (
                lines("        return self.cost"),
                lines(
                    '        if self.title.startswith("Free"):',
                    "            return 0.0",
                    "        return self.cost",
                ),
            ),
        ),
    ),
    Task(
        "12-shortest-path",
        "`shortest_path` must ignore edges with a negative weight.",
        lines(
            'test "negative edges":',
            '    graph = {"a": [("b", 5), ("c", -10)], "c": [("b", 1)], "b": []}',
            '    assert shortest_path(graph, "a", "b") == 5',
        ),
        (
            (
                lines("        for neighbor, weight in graph.get(node, []):"),
                lines(
                    "        for neighbor, weight in graph.get(node, []):",
                    "            if weight < 0:",
                    "                continue",
                ),
            ),
        ),
    ),
]
