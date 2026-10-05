from dataclasses import dataclass
from typing import Protocol


class Priced(Protocol):
    def price(self) -> float: ...

    def label(self) -> str: ...


@dataclass(frozen=True)
class Book:
    title: str
    cost: float

    def price(self) -> float:
        return self.cost

    def label(self) -> str:
        return f"Book: {self.title}"


@dataclass(frozen=True)
class Bundle:
    name: str
    items: tuple[Book, ...]
    discount: float

    def price(self) -> float:
        return sum(b.price() for b in self.items) * (1 - self.discount)

    def label(self) -> str:
        return f"Bundle: {self.name} ({len(self.items)} books)"


def report(products: list[Priced]) -> str:
    lines = [f"{p.label()}: {p.price():.2f}" for p in products]
    return "\n".join(lines)


def test_report() -> None:
    a = Book("A", 10.0)
    b = Book("B", 30.0)
    out = report([a, Bundle("AB", (a, b), 0.5)])
    assert out == "Book: A: 10.00\nBundle: AB (2 books): 20.00"
