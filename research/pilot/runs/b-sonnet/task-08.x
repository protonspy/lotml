trait Describe:
    fn describe(self) -> str

type Dog(name: str)
type Car(model: str, year: int)

impl Describe for Dog:
    fn describe(self) -> str:
        return f"Dog {self.name}"

impl Describe for Car:
    fn describe(self) -> str:
        return f"Car {self.model} ({self.year})"

fn describe_all(items: [dyn Describe]) -> str:
    return "\n".join([item.describe() for item in items])

test "describes a mixed list":
    items: [dyn Describe] = [Dog("Rex"), Car("Civic", 2020), Dog("Luna")]
    assert describe_all(items) == "Dog Rex\nCar Civic (2020)\nDog Luna"

test "a single item has no separator":
    items: [dyn Describe] = [Car("Fusca", 1975)]
    assert describe_all(items) == "Car Fusca (1975)"

test "an empty list gives an empty string":
    items: [dyn Describe] = []
    assert describe_all(items) == ""
