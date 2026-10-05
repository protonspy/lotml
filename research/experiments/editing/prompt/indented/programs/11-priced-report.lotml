trait Priced:
    fn price(self) -> f64
    fn label(self) -> str

type Book(title: str, cost: f64)
type Bundle(name: str, items: [Book], discount: f64)

impl Priced for Book:
    fn price(self) -> f64:
        return self.cost

    fn label(self) -> str:
        return f"Book: {self.title}"

impl Priced for Bundle:
    fn price(self) -> f64:
        return sum(b.price() for b in self.items) * (1.0 - self.discount)

    fn label(self) -> str:
        return f"Bundle: {self.name} ({len(self.items)} books)"

fn report(products: [dyn Priced]) -> str:
    lines = [f"{p.label()}: {p.price():.2f}" for p in products]
    return "\n".join(lines)

test "report":
    a = Book("A", 10.0)
    b = Book("B", 30.0)
    out = report([a, Bundle("AB", [a, b], 0.5)])
    assert out == "Book: A: 10.00\nBundle: AB (2 books): 20.00"
