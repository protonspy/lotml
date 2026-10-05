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
    return "\n".join([x.describe() for x in items])

test "describes a mixed list on separate lines":
    items: [dyn Describe] = [Dog("Rex"), Car("Civic", 2020), Dog("Fido")]
    assert describe_all(items) == "Dog named Rex\nCar Civic from 2020\nDog named Fido"

test "a single item has no separator":
    items: [dyn Describe] = [Car("Golf", 1999)]
    assert describe_all(items) == "Car Golf from 1999"

test "an empty list gives an empty string":
    items: [dyn Describe] = []
    assert describe_all(items) == ""
