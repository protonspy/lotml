# X language reference (variant A)

X is a statically typed language with Python-like syntax. Blocks are indented by 4 spaces
after a line ending in `:`. Comments start with `#`. Files use the extension `.x`.

## Declarations

```
fn add(a: int, b: int) -> int:
    return a + b
```

- Functions start with `fn`. Every parameter and the return type are annotated. A function
  that returns nothing omits `-> T`. Parameters may have defaults: `fn f(n: int = 0)`.
- `x = expr` declares an immutable local; its type is inferred. `x: T = expr` annotates it.
- `var x = expr` declares a mutable local. Only `var` locals may be reassigned or mutated
  (`x = ...`, `x += 1`, `xs.append(v)`, `d[k] = v`).
- Top-level items are `fn`, `type`, `trait`, `impl`, `use` and `test`. There is no global
  mutable state.

## Types

- Primitives: `int` (64-bit), `i8` `i16` `i32` `i64` `u8` `u16` `u32` `u64`, `f64`, `f32`,
  `bool`, `str`, `bytes`. Integer literals are `int`, `1.0` is `f64`. No implicit
  conversions: write `float(n)`, `int(x)`, `str(v)`.
- Collections: `[T]` list, `{K: V}` dict, `(A, B)` tuple. Literals are as in Python:
  `[1, 2]`, `{"a": 1}`, `(1, "x")`.
- `T?` is an optional: a `T` or `none`. It is the only type that admits `none`.
- `T ! E` is a result: a `T`, or an error of type `E`.
- Generics use brackets: `[T]`, `Stack[int]`, `fn first[T](xs: [T]) -> T?`.
  Bounds: `fn max_of[T: Ord](xs: [T]) -> T?`.

## Records and sum types

```
type User(name: str, age: int, email: str? = none)
type Shape = Circle(r: f64) | Rect(w: f64, h: f64) | Empty
```

- A record is declared in one line. Construct it positionally or by name:
  `User("ana", 30)`, `User(name="ana", age=30)`. Fields are read with `u.name`.
- A sum type lists its variants; each variant is a constructor in scope (`Circle(1.0)`,
  `Empty`). Variants without fields are written without parentheses.
- Records are values: assigning or passing one copies it. Field updates require a `var`.

## Pattern matching

```
fn area(s: Shape) -> f64:
    match s:
        Circle(r):
            return 3.14159 * r * r
        Rect(w, h):
            return w * h
        Empty:
            return 0.0
```

- Each arm is a pattern followed by `:` and a block. Patterns: a variant with
  sub-patterns `Rect(w, h)`, a bare variant `Empty`, a literal, `none`, `Ok(v)`, `Err(e)`,
  or `_` (anything).
- `match` must be exhaustive; the compiler rejects a missing case.

## Optionals

- `none` is the empty value. `if x:` on a `T?` tests that it is not `none` and narrows `x`
  to `T` inside the block. `if` otherwise only accepts `bool`: there is no truthiness of
  numbers, strings or collections (write `len(xs) > 0`, `s == ""`).
- `x or default` yields the value inside `x`, or `default` when `x` is `none`.
- `x == none` compares with `none`.

## Errors

```
type ParseErr = Empty | NotNumber(text: str)

fn parse(s: str) -> int ! ParseErr:
    if s == "":
        fail Empty
    n = s.to_int() or fail NotNumber(s)
    return n

fn double(s: str) -> int ! ParseErr:
    n = parse(s)?
    return n * 2
```

- `fail e` returns the error `e` from the current function.
- `expr?` unwraps a result, or returns its error from the current function (the error
  types must match).
- `x or fail e` unwraps an optional or fails with `e`.
- Results match with `Ok(v)` and `Err(e)` patterns.
- A function that can fail but has no value to return is declared `-> none ! E`.
- There are no exceptions. A panic happens only on broken invariants (index out of
  range, failed `assert`).

## Control flow

`if` / `elif` / `else`, `while`, `for x in iterable`, `for i, x in enumerate(xs)`,
`break`, `continue`, `return`, `pass`. Conditional expression: `a if cond else b`.
List, dict and generator comprehensions work as in Python. F-strings work as in Python:
`f"{name}: {value:.2f}"`.

## Lambdas

`x => expr` is an anonymous function of one argument: `sorted(xs, key=u => u.age)`.

## Methods and traits

```
type Counter(count: int)

impl Counter:
    fn get(self) -> int:
        return self.count

    fn bump(var self):
        self.count += 1

trait Show:
    fn show(self) -> str

impl Show for Counter:
    fn show(self) -> str:
        return f"Counter({self.count})"
```

- Methods live in `impl Type:` blocks. `self` is the receiver; `var self` lets a method
  mutate it. Call methods with `c.get()`.
- `trait` declares method signatures. `impl Trait for Type:` implements them. Generic
  bounds use traits: `fn show_all[T: Show](xs: [T]) -> str`. A list of mixed types that
  share a trait is `[dyn Show]`.
- There are no classes and no inheritance.

## Modules

`use math.{sqrt, pi}` imports names. `use strings` imports a module, used as `strings.f`.

## Tests

```
test "parse rejects empty":
    assert parse("") == fail Empty
    assert double("21")? == 42
```

- `test "name":` blocks sit next to the code. Inside, `assert cond` checks a condition,
  `expr?` fails the test on an error, and `== fail e` compares a result with an error.

## Built-ins

`print`, `len`, `range`, `enumerate`, `zip`, `sorted`, `reversed`, `sum`, `min`, `max`,
`abs`, `str`, `int`, `float`, `round`.

- `str`: `split`, `strip`, `lower`, `upper`, `startswith`, `endswith`, `replace`, `join`,
  `isalpha`, `isdigit`, `find(sub) -> int?`, `split_once(sep) -> (str, str)?`,
  `to_int() -> int?`, `to_float() -> f64?`.
- `[T]`: `append`, `extend`, `insert`, `pop() -> T?`, `last() -> T?`,
  `find(pred) -> T?`, `contains(v) -> bool`, `sort()`, indexing `xs[i]`, slicing `xs[a:b]`.
- `{K: V}`: `get(k) -> V?`, `get(k, default) -> V`, `keys()`, `values()`, `items()`,
  `pop(k) -> V?`, `k in d`.
- `Heap[T]`: `Heap(items)`, `push(v)`, `pop_min() -> T`, `len(h)`.

## Not in the language

`class`, `def`, `None`, `lambda`, `import`, `from`, `raise`, `try`, `except`, `with`,
decorators, `async`/`await`, `global`, `*args`/`**kwargs`, truthiness of non-optionals.
