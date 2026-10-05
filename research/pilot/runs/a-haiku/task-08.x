type Dog(name: str)
type Car(model: str, year: int)

trait Describe:
    fn describe(self) -> str

impl Describe for Dog:
    fn describe(self) -> str:
        return f"Dog: {self.name}"

impl Describe for Car:
    fn describe(self) -> str:
        return f"Car: {self.model} ({self.year})"

fn join_descriptions(items: [dyn Describe]) -> str:
    descriptions = [item.describe() for item in items]
    return "\n".join(descriptions)

test "describe":
    dog = Dog("Buddy")
    car = Car("Tesla", 2023)
    items: [dyn Describe] = [dog, car]
    result = join_descriptions(items)
    assert result == "Dog: Buddy\nCar: Tesla (2023)"
