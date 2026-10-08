# lotml language reference

lotml is a statically typed language with Python's syntax wherever its meaning is Python's. Files
use the `.lot` extension, or `.lotml`. Blocks are indented by 4 spaces after a line ending in `:`;
comments start with `#`. Each section is an example to copy. Lines marked **Not Python** are
where lotml means something different from what the same Python would mean.

## Functions and locals

```
fn area(width: f64, height: f64) -> f64:
    return width * height

fn greet(name: str, punctuation: str = "!") -> str:
    return f"hello, {name}{punctuation}"

fn log(message: str):
    """Print one line of the run's log."""
    print(message)
```

- Every parameter and the return type are annotated. A function with no `-> T` returns `None`,
  the unit type.
- A string on the first line of a body documents the function, as in Python.
- `x = expr` declares an immutable local with an inferred type; `x: T = expr` annotates it.

```
fn total(prices: [int]) -> int:
    var sum = 0
    for p in prices:
        sum += p
    return sum
```

- **Not Python:** only a `var` may be reassigned or mutated (`x = …`, `x += 1`, `xs.append(v)`,
  `d[k] = v`, `p.x = 1`). Assigning to an immutable is a compile error, not a new binding.
  Declare it `var` at its first assignment.
- There is no global mutable state. Top-level items are `fn`, `type`, `trait`, `impl`,
  `import`/`from` and `test`.

## Numbers

```
n = 7
half = n / 2           # 3.5: `/` always returns f64, as in Python
whole = n // 2         # 3: floor division
rest = -7 % 2          # 1: the sign of the divisor, as in Python
mean = float(sum(xs)) / float(len(xs))
k = int(3.9)           # 3: truncates toward zero
bits = (n & 6) | (1 << 4) ^ ~n
```

- `int` is a 64-bit signed integer (alias of `i64`); also `i8` `i16` `i32` `u8` `u16` `u32` `u64`,
  `f64`, `f32`. Integer literals are `int`, or the sized type expected where they fit
  (`limit: u8 = 200`); `1.0` and `1e9` are `f64`. `i32(n)`, `u8(n)` and the rest convert,
  stopping the program when the value does not fit.
- **Not Python:** `int` does not grow. Overflow stops the program with an error, in every build.
  Use `wrapping_add(a, b)`, `wrapping_mul(a, b)` for modular arithmetic.
- **Not Python:** no implicit conversions except `/`, which converts both integers to `f64`.
  `1 + 2.0` is a type error: write `float(1) + 2.0`. `int(x)` and `float(n)` convert numbers,
  `str(v)` renders any value, and text converts with `to_int()` and `to_float()`.
- `**`, `abs`, `min`, `max`, `round`, comparisons (chained as in Python: `0 <= i < n`) and the bit
  operators `&` `|` `^` `~` `<<` `>>` work as in Python.

## Strings

```
s = "Hello, World"
s.lower()                     # "hello, world"
s.split(", ")                 # ["Hello", "World"]
", ".join(["a", "b"])         # "a, b"
s[0]                          # "H": indexing a str gives a str
s[1:4]                        # "ell"
f"{name} is {age} years old, {ratio:.2f}"
s.find("World")               # 7, or None when absent: the type is int?
"42".to_int()                 # 42, or None when it is not a number: int?
```

- `str` is immutable text. Methods: `split`, `strip`, `lstrip`, `rstrip`, `lower`, `upper`,
  `startswith`, `endswith`, `replace`, `join`, `count`, `isalpha`, `isdigit`, `isspace`,
  `isupper`, `islower`, `find(sub) -> int?`, `split_once(sep) -> (str, str)?`,
  `to_int() -> int?`, `to_float() -> f64?`. `ord(c)`, `chr(n)`, `len(s)`, `c in s`.
- An f-string field's spec after `:` follows Python's format mini-language for the value's type,
  and is checked: one the value would refuse, as `{s:d}` for a `str`, is an error (E0223).

## Collections

```
xs: [int] = [3, 1, 2]
var ys = [1, 2]
ys.append(3)
ages: {str: int} = {"ana": 30}
seen: {int} = set()
pair: (str, int) = ("ana", 30)
name, age = pair
squares = [x * x for x in xs if x > 1]
index = {w: len(w) for w in words}
```

- `[T]` list, `{K: V}` dict, `{T}` set, `(A, B)` tuple. Literals as in Python; the empty set is
  `set()`.
- List: `append`, `extend`, `insert`, `pop() -> T?`, `last() -> T?`, `find(pred) -> T?`,
  `contains(v)`, `sort()`, `reverse()`, `index(v) -> int?`, `count(v)`, `xs[i]`, `xs[a:b]`,
  `xs[-1]`. Indexing out of range stops the program.
- Dict: `get(k) -> V?`, `get(k, default) -> V`, `keys()`, `values()`, `items()`,
  `pop(k) -> V?`, `k in d`, `d[k]` (stops the program when `k` is absent).
- Set: `add`, `remove`, `discard`, `v in s`, `|`, `&`, `-`.
- **Not Python:** collections are values. `ys = xs` copies; changing `ys` never changes `xs`.

## Records

```
type Point(x: f64, y: f64)
type User(name: str, age: int, email: str? = None)

p = Point(1.0, 2.0)
u = User(name="ana", age=30)
var q = p
q.x = 5.0              # p.x is still 1.0
```

- A record is declared on one line and built positionally or by name; fields are read with
  `p.x`. Records compare with `==` field by field.
- **Not Python:** records are values: `q = p` copies. A field changes only through a `var`.

## Sum types and `match`

```
type Shape = Circle(r: f64) | Rect(w: f64, h: f64) | Empty
type Expr = Num(int) | Add(Expr, Expr) | Neg(Expr)

fn area(s: Shape) -> f64:
    match s:
        case Circle(r):
            return 3.14159 * r * r
        case Rect(w, h):
            return w * h
        case Empty:
            return 0.0

fn eval(e: Expr) -> int:
    match e:
        case Num(n):
            return n
        case Add(a, b):
            return eval(a) + eval(b)
        case Neg(inner):
            return -eval(inner)
```

- Variants are constructors in scope (`Circle(1.0)`, `Empty`); fields are named or positional.
- Patterns: a variant with sub-patterns, a bare variant, a literal, `None`, `Ok(v)`, `Err(e)`, a
  name that binds, `_`.
- **Not Python:** `match` must cover every case; a missing variant is a compile error.

## Optionals

```
fn find_email(users: [User], name: str) -> str?:
    for u in users:
        if u.name == name:
            return u.email
    return None

email = find_email(users, "ana")
if email is not None:
    send(email)             # here email is a str
shown = email ?? "no email"
```

- `T?` is a `T` or `None`; it is the only type that admits `None`. `x is not None` narrows `x`
  to `T` inside the block.
- `x ?? default` is the value inside `x`, or `default` when `x` is `None`.
- **Not Python:** no truthiness. `if` and `while` accept only `bool`: write `len(xs) > 0`,
  `s != ""`, `x is not None`, `n != 0`. `and`, `or` and `not` take and return `bool`.

## Errors as values

```
type ParseErr = Empty | NotNumber(text: str) | Negative(value: int)

fn parse(s: str) -> int ! ParseErr:
    if s == "":
        fail Empty
    n = s.to_int() ?? fail NotNumber(s)
    if n < 0:
        fail Negative(n)
    return n

fn double(s: str) -> int ! ParseErr:
    n = parse(s)?
    return n * 2

fn check(s: str) -> None ! ParseErr:
    parse(s)?

fn describe(s: str) -> str:
    match parse(s):
        case Ok(n):
            return f"number {n}"
        case Err(NotNumber(text)):
            return f"not a number: {text}"
        case Err(_):
            return "invalid"
```

- `T ! E` is a result: a `T`, or an error of type `E`. `fail e` returns the error `e`.
- `expr?` unwraps a result, or returns its error from the current function.
- `x ?? fail e` unwraps an optional or fails with `e`.
- A function that can fail and has no value to return is `-> None ! E`; it ends with no
  `return`, or with a bare `return`.
- **Not Python:** there are no exceptions: no `raise`, `try` or `except`. A failure is a value
  of the return type. The program stops only on a broken invariant: an index out of range, an
  absent dict key, overflow, division by zero, a failed `assert`, `todo()`.

## Unfinished code

```
fn schedule(jobs: [Job]) -> Plan:
    return todo()
```

- `todo()` fills any hole whatever its type, so an unfinished program still compiles. Running
  it stops the program with "not implemented".

## Parameters: read, `inout`, `sink`

```
fn add_all(inout xs: [int], values: [int]):
    for v in values:
        xs.append(v)

fn consume(sink log: [str]) -> int:
    return len(log)

var xs = [1]
add_all(&xs, [2, 3])         # xs is [1, 2, 3]
n = consume(lines)           # lines can no longer be used
```

- A parameter is read-only by default; to change a copy, declare `var mine = param`.
- **Not Python:** a function cannot change its caller's value through a plain parameter: it
  gets a copy. To change it, declare the parameter `inout` and pass `&x`, where `x` is a `var`.
- `sink` takes the value over; the caller may not use it after the call.

## Methods and traits

```
type Counter(count: int)

impl Counter:
    fn new() -> Counter:
        return Counter(0)

    fn get(self) -> int:
        return self.count

    fn bump(inout self):
        self.count += 1

trait Show:
    fn show(self) -> str

impl Show for Counter:
    fn show(self) -> str:
        return f"Counter({self.count})"

var c = Counter.new()
c.bump()
print(c.show())

fn show_all[T: Show](items: [T]) -> str:
    return "\n".join([i.show() for i in items])

fn show_mixed(items: [dyn Show]) -> str:
    return "\n".join([i.show() for i in items])
```

- Methods live in `impl Type:`. `self` reads the receiver; `inout self` changes it, and the
  receiver must be a `var`. A function in `impl` without `self` is called on the type.
- `trait` declares method signatures; `impl Trait for Type:` implements them. `[dyn Show]` holds
  values of different types sharing a trait.
- **Not Python:** no classes and no inheritance.

## Generics

```
type Stack[T](items: [T])

impl Stack[T]:
    fn push(inout self, item: T):
        self.items.append(item)

    fn pop(inout self) -> T?:
        return self.items.pop()

fn first[T](xs: [T]) -> T?:
    if len(xs) == 0:
        return None
    return xs[0]

fn largest[T: Ord](xs: [T]) -> T?:
    return max(xs) if len(xs) > 0 else None
```

- Type parameters go in brackets; a bound names a trait: `Ord`, `Eq`, `Hash`, `Show`, or your own.

## Control flow

```
for i, x in enumerate(xs):
    if x < 0:
        continue
    elif x > 100:
        break
    else:
        print(i)
while n > 1:
    n = n // 2 if n % 2 == 0 else 3 * n + 1
for k, v in sorted(ages.items()):
    print(f"{k}: {v}")
pairs = sorted(users, key=lambda u: u.age)
```

- `if`/`elif`/`else`, `while`, `for x in iterable`, `break`, `continue`, `pass`, `return`;
  `a if cond else b`; list, dict, set and generator comprehensions; `lambda x: expr`.
- **Not Python:** a `lambda` captures copies of the values it uses.

## Concurrency

```
fn squares(xs: [int]) -> [int]:
    return parallel([lambda: x * x for x in xs])
```

- `parallel(tasks)` runs each task — a function with no parameters — at once, waits for them
  all and returns their results in order. A call that blocks holds up only its own task.
- **Not Python:** no function is marked `async` and nothing is awaited.

## Modules and the prelude

```
from math import sqrt, pi
import math

d = sqrt(2.0) * pi
steps = math.floor(d)
```

- `from m import a, b` or `import m`; there is no `import *`.
- The prelude needs no import: `print`, `len`, `range`, `enumerate`, `zip`, `sorted`,
  `reversed`, `sum`, `min`, `max`, `abs`, `any`, `all`, `round`, `int`, `float`, `str`, `bool`,
  `ord`, `chr`, `set`, `list`, `Heap`, `todo`, `Ok`, `Err`, `parallel`, `PyError`.
- `Heap[T]`: `Heap(items)`, `push(v)`, `pop_min() -> T?`, `peek() -> T?`, `len(h)`.
- `math`: `sqrt`, `floor`, `ceil`, `pow`, `log`, `exp`, `sin`, `cos`, `pi`, `inf`, `gcd`,
  `isqrt`.
- A Python module is imported by its origin, `py.`, once `lotml bind <module>` has written its
  interface from the module's stub, `bindings/py.<module>.lotmli`: `from py.textwrap import
  dedent`, then `text = dedent(raw)?`; or `import py.textwrap`, then
  `py.textwrap.dedent(raw)?`. A bare `import textwrap` names a LotML module only. Each of its
  functions returns `T ! PyError`, since any call into Python can fail; `PyError(kind, message)`
  is in the prelude.
- A C library is imported from its interface, written by hand in `bindings/c.<library>.lotmli`:
  `from c.m import cos`. Its functions take and return numbers, `bool` and (taken only) `str`,
  and cannot fail.

## Tests

```
test "parse":
    assert parse("42") == Ok(42)
    assert parse("") == Err(Empty)
    assert double("21")? == 42
```

- `test "name":` blocks sit next to the code. `assert cond` checks a `bool`; `expr?` fails the
  test on an error; a result compares with `== Ok(v)` or `== Err(e)`.

## Not in the language

`class`, `def`, `raise`, `try`, `except`, `finally`, `with`, `yield`, decorators,
`async`/`await`, `global`, `nonlocal`, `*args`/`**kwargs`, `isinstance`, `Any`, `Optional[…]`,
`List[…]`, dunder names, truthiness of non-`bool` values.
