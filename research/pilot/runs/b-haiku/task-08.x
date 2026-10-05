trait Describe:
    fn describe(self) -> str

type Dog(name: str)
type Car(model: str, year: int)

impl Describe for Dog:
    fn describe(self) -> str:
        return f"Dog named {self.name}"

impl Describe for Car:
    fn describe(self) -> str:
        return f"{self.year} {self.model}"

fn describe_all(items: [dyn Describe]) -> str:
    descriptions = []
    for item in items:
        descriptions.append(item.describe())
    return "\n".join(descriptions)

test "describe animals and cars":
    items: [dyn Describe] = [Dog("Buddy"), Car("Tesla", 2023)]
    result = describe_all(items)
    assert "Dog named Buddy" in result
    assert "2023 Tesla" in result
