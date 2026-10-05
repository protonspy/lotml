from dataclasses import dataclass
from math import pi, sqrt


@dataclass
class Circle:
    r: float


@dataclass
class Rect:
    w: float
    h: float


@dataclass
class Triangle:
    a: float
    b: float
    c: float


type Shape = Circle | Rect | Triangle


def area(s: Shape) -> float:
    match s:
        case Circle(r):
            return pi * r * r
        case Rect(w, h):
            return w * h
        case Triangle(a, b, c):
            p = (a + b + c) / 2
            return sqrt(p * (p - a) * (p - b) * (p - c))


def total_area(shapes: list[Shape]) -> float:
    return sum(area(s) for s in shapes)


def test_total_area() -> None:
    shapes: list[Shape] = [Rect(2, 3), Triangle(3, 4, 5)]
    assert total_area(shapes) == 12
