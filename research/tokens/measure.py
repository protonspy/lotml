"""Measure the paired corpus and isolated constructs; write results.md and results.json."""

import json
import re
import statistics
from pathlib import Path

from counter import TOKENIZERS, count

HERE = Path(__file__).parent
CORPUS = HERE / "corpus"
VARIANTS = {"typed": "typed.py", "a": "a.x", "b": "b.x"}

# Each entry isolates one construct: (label, [(form name, text), ...]).
# The first form is the baseline the others are compared against.
MICRO = [
    (
        "function keyword",
        [
            ("def", "def f(x: int) -> int:\n"),
            ("fn", "fn f(x: int) -> int:\n"),
        ],
    ),
    (
        "optional field",
        [
            ("Optional[str]", "    email: Optional[str] = None\n"),
            ("str | None", "    email: str | None = None\n"),
            ("str? = none", "    email: str? = none\n"),
            ("str? = None", "    email: str? = None\n"),
        ],
    ),
    (
        "list type",
        [
            ("list[int]", "xs: list[int]\n"),
            ("[int]", "xs: [int]\n"),
        ],
    ),
    (
        "dict type",
        [
            ("dict[str, int]", "d: dict[str, int]\n"),
            ("{str: int}", "d: {str: int}\n"),
        ],
    ),
    (
        "tuple type",
        [
            ("tuple[str, int]", "p: tuple[str, int]\n"),
            ("(str, int)", "p: (str, int)\n"),
        ],
    ),
    (
        "lambda",
        [
            ("lambda", "key=lambda p: p.age\n"),
            ("=>", "key=p => p.age\n"),
        ],
    ),
    (
        "import",
        [
            ("from-import", "from a.b import c, d\n"),
            ("use {}", "use a.b.{c, d}\n"),
        ],
    ),
    (
        "optional default",
        [
            ("or", "    x = y or 0\n"),
            ("??", "    x = y ?? 0\n"),
        ],
    ),
    (
        "optional test",
        [
            ("is not None", "    if x is not None:\n"),
            ("if x", "    if x:\n"),
            ("!= none", "    if x != none:\n"),
        ],
    ),
    (
        "fallible signature",
        [
            ("-> User (raises)", "def find(name: str) -> User:\n"),
            ("-> User ! E", "fn find(name: str) -> User ! LookupErr:\n"),
            ("-> Result[User, E]", "fn find(name: str) -> Result[User, LookupErr]:\n"),
        ],
    ),
    (
        "error exit",
        [
            ("raise", "        raise NotFound(name)\n"),
            ("fail", "        fail NotFound(name)\n"),
            ("return Err", "        return Err(NotFound(name))\n"),
        ],
    ),
    (
        "record",
        [
            ("@dataclass", "@dataclass\nclass User:\n    name: str\n    age: int\n"),
            ("type (...)", "type User(name: str, age: int)\n"),
        ],
    ),
    (
        "frozen record",
        [
            (
                "@dataclass(frozen)",
                "@dataclass(frozen=True)\nclass P:\n    x: float\n    y: float\n",
            ),
            ("type (...)", "type P(x: f64, y: f64)\n"),
        ],
    ),
    (
        "match arm",
        [
            ("case", "        case Circle(r):\n"),
            ("bare", "        Circle(r):\n"),
        ],
    ),
    (
        "mutable binding",
        [
            ("x = 1", "    x = 1\n"),
            ("var x = 1", "    var x = 1\n"),
            ("let mut x = 1", "    let mut x = 1\n"),
        ],
    ),
    (
        "empty-list test",
        [
            ("if xs", "    if xs:\n"),
            ("len(xs) > 0", "    if len(xs) > 0:\n"),
        ],
    ),
    (
        "block delimiters",
        [
            ("indent", "if ok:\n    run()\nnext()\n"),
            ("braces", "if ok {\n    run()\n}\nnext()\n"),
        ],
    ),
    (
        "indent width",
        [
            ("4 spaces", "def f():\n    if x:\n        return y\n"),
            ("tab", "def f():\n\tif x:\n\t\treturn y\n"),
            ("2 spaces", "def f():\n  if x:\n    return y\n"),
        ],
    ),
]

# Full words against the abbreviations and symbols the original study warns about.
WORDS = [
    ("count", "cnt"),
    ("index", "idx"),
    ("value", "val"),
    ("message", "msg"),
    ("number", "num"),
    ("result", "res"),
    ("return", "rtn"),
    ("buffer", "buf"),
    ("lambda", "λ"),
    ("->", "→"),
    ("<=", "≤"),
    ("!=", "≠"),
    ("not", "¬"),
    ("reshape", "⍴"),
    ("outer_product", "∘.×"),
    ("reverse", "⌽"),
]


def tokens_by_tokenizer(text: str) -> dict[str, int]:
    return {name: count(name, text) for name in TOKENIZERS}


def strip_indentation(text: str) -> str:
    return re.sub(r"^[ \t]+", "", text, flags=re.MULTILINE)


def measure_corpus() -> dict:
    tasks = {}
    for task_dir in sorted(p for p in CORPUS.iterdir() if p.is_dir()):
        task = {}
        for variant, filename in VARIANTS.items():
            path = task_dir / filename
            if not path.exists():
                continue
            text = path.read_text(encoding="utf-8")
            task[variant] = {
                "lines": sum(1 for line in text.splitlines() if line.strip()),
                "chars": len(text),
                "tokens": tokens_by_tokenizer(text),
                "indent_tokens": {
                    name: count(name, text) - count(name, strip_indentation(text))
                    for name in TOKENIZERS
                },
            }
        tasks[task_dir.name] = task
    return tasks


def ratio(tasks: dict, variant: str, name: str, names: list[str]) -> float:
    total = sum(tasks[t][variant]["tokens"][name] for t in names)
    base = sum(tasks[t]["typed"]["tokens"][name] for t in names)
    return total / base


def render(tasks: dict, micro: list, words: list) -> str:
    names = list(TOKENIZERS)
    suite = [t for t in tasks if not t.startswith("00-")]
    out = [
        "# Token measurements",
        "",
        "Generated by `measure.py` — do not edit by hand.",
        "",
    ]

    out += [
        "## Corpus totals (12 paired tasks, tests included)",
        "",
        "| tokenizer | typed Python | variant A | variant B | A / typed | B / typed |",
        "| --- | ---: | ---: | ---: | ---: | ---: |",
    ]
    for name in names:
        t, a, b = (
            sum(tasks[x][v]["tokens"][name] for x in suite) for v in ("typed", "a", "b")
        )
        out.append(f"| {name} | {t} | {a} | {b} | {a / t:.3f} | {b / t:.3f} |")
    lines = {v: sum(tasks[x][v]["lines"] for x in suite) for v in VARIANTS}
    chars = {v: sum(tasks[x][v]["chars"] for x in suite) for v in VARIANTS}
    out += [
        "",
        (
            f"Non-blank lines: typed {lines['typed']}, A {lines['a']}, B {lines['b']}. "
            f"Characters: typed {chars['typed']}, A {chars['a']}, B {chars['b']}."
        ),
        "",
    ]

    out += [
        "## Per task, A / typed and B / typed (median across tokenizers)",
        "",
        "| task | typed tokens (median) | A / typed | B / typed |",
        "| --- | ---: | ---: | ---: |",
    ]
    for task in suite:
        base = statistics.median(tasks[task]["typed"]["tokens"].values())
        ra = statistics.median(ratio(tasks, "a", n, [task]) for n in names)
        rb = statistics.median(ratio(tasks, "b", n, [task]) for n in names)
        out.append(f"| {task} | {base:.0f} | {ra:.3f} | {rb:.3f} |")
    out.append("")

    orig = tasks["00-original-example"]
    out += [
        "## The original study's own example (verbatim, no tests)",
        "",
        "| tokenizer | Python as written | proposal as written | ratio |",
        "| --- | ---: | ---: | ---: |",
    ]
    for name in names:
        t, a = orig["typed"]["tokens"][name], orig["a"]["tokens"][name]
        out.append(f"| {name} | {t} | {a} | {a / t:.3f} |")
    out += [
        "",
        f"Non-blank lines: Python {orig['typed']['lines']}, proposal {orig['a']['lines']}.",
        "",
    ]

    out += [
        "## Share of tokens spent on indentation",
        "",
        "| tokenizer | typed Python | variant A |",
        "| --- | ---: | ---: |",
    ]
    for name in names:
        shares = []
        for v in ("typed", "a"):
            ind = sum(tasks[x][v]["indent_tokens"][name] for x in suite)
            tot = sum(tasks[x][v]["tokens"][name] for x in suite)
            shares.append(f"{100 * ind / tot:.1f}%")
        out.append(f"| {name} | {shares[0]} | {shares[1]} |")
    out.append("")

    out += [
        "## Isolated constructs",
        "",
        "Tokens per form; `Δ` is the median difference against the first form.",
        "",
        "| construct | form | " + " | ".join(names) + " | Δ median |",
        "| --- | --- | " + " | ".join("---:" for _ in names) + " | ---: |",
    ]
    for label, forms in micro:
        base = tokens_by_tokenizer(forms[0][1])
        for form, text in forms:
            row = tokens_by_tokenizer(text)
            delta = statistics.median(row[n] - base[n] for n in names)
            out.append(
                f"| {label} | `{form}` | "
                + " | ".join(str(row[n]) for n in names)
                + f" | {delta:+g} |"
            )
    out.append("")

    out += [
        "## Words against abbreviations and symbols (with a leading space)",
        "",
        "| word | tokens (min–max) | short form | tokens (min–max) |",
        "| --- | --- | --- | --- |",
    ]
    for word, short in words:
        w = tokens_by_tokenizer(" " + word).values()
        s = tokens_by_tokenizer(" " + short).values()
        out.append(f"| `{word}` | {min(w)}–{max(w)} | `{short}` | {min(s)}–{max(s)} |")
    out.append("")
    return "\n".join(out)


def main() -> None:
    tasks = measure_corpus()
    micro = {
        label: {form: tokens_by_tokenizer(text) for form, text in forms}
        for label, forms in MICRO
    }
    words = {w: tokens_by_tokenizer(" " + w) for pair in WORDS for w in pair}
    (HERE / "results.json").write_text(
        json.dumps({"corpus": tasks, "micro": micro, "words": words}, indent=1),
        encoding="utf-8",
    )
    (HERE / "results.md").write_text(render(tasks, MICRO, WORDS), encoding="utf-8")


if __name__ == "__main__":
    main()
