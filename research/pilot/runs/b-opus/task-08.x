# Task 8: a describe interface implemented by a dog and a car.

trait Describe:
    fn describe(self) -> str

type Dog(name: str)
type Car(model: str, year: int)

impl Describe for Dog:
    fn describe(self) -> str:
        return f"Dog named {self.name}"

impl Describe for Car:
    fn describe(self) -> str:
        return f"Car {self.model} from {self.year}"

fn describe_all(items: [dyn Describe]) -> str:
    return "\n".join([it.describe() for it in items])

test "joins the descriptions of a mixed list":
    items: [dyn Describe] = [Dog("Rex"), Car("Beetle", 1967), Dog("Fido")]
    assert describe_all(items) == "Dog named Rex\nCar Beetle from 1967\nDog named Fido"

test "empty list gives empty text":
    items: [dyn Describe] = []
    assert describe_all(items) == ""
