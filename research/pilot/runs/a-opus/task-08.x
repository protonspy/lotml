# Task 8: describe trait over a mixed list

trait Describe:
    fn describe(self) -> str

type Dog(name: str)
type Car(model: str, year: int)

impl Describe for Dog:
    fn describe(self) -> str:
        return f"Dog named {self.name}"

impl Describe for Car:
    fn describe(self) -> str:
        return f"{self.model} car from {self.year}"

fn describe_all(items: [dyn Describe]) -> str:
    return "\n".join([item.describe() for item in items])

test "describes each kind":
    assert Dog("Rex").describe() == "Dog named Rex"
    assert Car("Civic", 2020).describe() == "Civic car from 2020"

test "joins descriptions of a mixed list with newlines":
    items: [dyn Describe] = [Dog("Rex"), Car("Civic", 2020), Dog("Bolt")]
    assert describe_all(items) == "Dog named Rex\nCivic car from 2020\nDog named Bolt"

test "empty list gives an empty string":
    items: [dyn Describe] = []
    assert describe_all(items) == ""
