"""The harness task set: functions to write in lotml, with hidden tests."""

import re
from dataclasses import dataclass, field
from typing import Any

from lotml_harness.tasks import types, values

COMPARISONS = ("eq", "approx", "set", "lines")
"""How a case compares the result: exactly, within 1e-6, as sets, or as output lines."""


@dataclass
class Case:
    args: list[Any]
    expected: Any
    compare: str = "eq"


@dataclass
class Task:
    id: str
    source: str
    name: str
    params: list[tuple[str, types.Type]]
    returns: types.Type
    doc: str
    tests: list[Case]
    canonical: str = ""
    meta: dict[str, Any] = field(default_factory=dict)

    def signature(self, variant: str) -> str:
        params = ", ".join(f"{n}: {t.render(variant)}" for n, t in self.params)
        arrow = "" if self.returns == types.Unit() else f" -> {self.returns.render(variant)}"
        return f"fn {self.name}({params}){arrow}:"

    def prompt(self, variant: str) -> str:
        """The signature and docstring the model completes."""
        doc = re.sub(r"\bNone\b", "none", self.doc) if variant == "a" else self.doc
        lines = self.signature(variant) + "\n"
        if doc:
            lines += f'    """{doc}"""\n'
        return lines

    def to_json(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "source": self.source,
            "name": self.name,
            "params": [[n, t.render("b")] for n, t in self.params],
            "returns": self.returns.render("b"),
            "doc": self.doc,
            "tests": [
                {
                    "args": [
                        values.to_json(a, t)
                        for a, (_, t) in zip(case.args, self.params, strict=True)
                    ],
                    "expected": values.to_json(case.expected, self.returns),
                    "compare": case.compare,
                }
                for case in self.tests
            ],
            "canonical": self.canonical,
            "meta": self.meta,
        }

    @classmethod
    def from_json(cls, record: dict[str, Any]) -> "Task":
        params = [(n, types.parse(t)) for n, t in record["params"]]
        returns = types.parse(record["returns"])
        tests = [
            Case(
                args=[
                    values.from_json(a, t) for a, (_, t) in zip(case["args"], params, strict=True)
                ],
                expected=values.from_json(case["expected"], returns),
                compare=case["compare"],
            )
            for case in record["tests"]
        ]
        return cls(
            id=record["id"],
            source=record["source"],
            name=record["name"],
            params=params,
            returns=returns,
            doc=record["doc"],
            tests=tests,
            canonical=record.get("canonical", ""),
            meta=record.get("meta", {}),
        )
