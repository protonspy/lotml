//! The Python backend (R13, R26; adr:0008): each program is compiled, imported by a real Python
//! and exercised; what is asserted is what lotml says the program does.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use lotml_py::{RUNTIME, compile, python};

/// A copy in a compiled module: a call of the runtime's `copy`, as the JSON string in the stub
/// spells it.
fn copies(module: &str) -> bool {
    module.contains(r#"\"attr\":\"copy\""#) || module.contains(r#"\"attr\":\"shallow\""#)
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("backend").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

/// Compile `source` as `prog.lotml` and run `script` with the module imported as `m`.
fn run(name: &str, source: &str, script: &str) -> Output {
    let dir = scratch(name);
    let path = dir.join("prog.lotml");
    std::fs::write(&path, source).expect("the source");
    let module = compile(source, &path).unwrap_or_else(|d| panic!("{source}\ndoes not compile: {d:#?}"));
    std::fs::write(dir.join("prog.py"), module).expect("the module");
    std::fs::write(dir.join("lotml_rt.py"), RUNTIME).expect("the runtime");
    let python = python().expect("a Python interpreter: python3, python, py -3 or LOTML_PYTHON");
    Command::new(&python[0])
        .args(&python[1..])
        .arg("-c")
        .arg(format!("import prog as m\n{script}"))
        .current_dir(&dir)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .output()
        .expect("Python runs")
}

/// The printed output of a run that must succeed.
fn prints(name: &str, source: &str, script: &str) -> String {
    let out = run(name, source, script);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{source}\n{script}\nfailed:\n{stderr}");
    String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n").trim_end().to_string()
}

// R13: errors point at the lotml source -----------------------------------------------------

#[test]
fn a_traceback_names_the_lotml_line_and_underlines_the_expression() {
    let out = run("traceback", "fn ratio(d: int?) -> int:\n    return 10 // (d ?? 0)\n", "m.ratio(None)");
    let stderr = String::from_utf8_lossy(&out.stderr).replace("\r\n", "\n");
    assert!(!out.status.success());
    assert!(stderr.contains("prog.lotml\", line 2, in ratio"), "{stderr}");
    assert!(stderr.contains("    return 10 // (d ?? 0)\n           ^^^^^^^^^^^^^^"), "{stderr}");
    assert!(stderr.contains("ZeroDivisionError"), "{stderr}");
}

#[test]
fn a_program_that_does_not_check_does_not_compile() {
    let errors = compile("fn f() -> int:\n    return \"a\"\n", Path::new("f.lotml")).expect_err("a type error");
    assert_eq!(errors.iter().map(|d| d.code).collect::<Vec<_>>(), vec!["E0204"]);
}

// R26: one overflow semantics, a trap ------------------------------------------------------

const ARITHMETIC: &str = "fn add(a: int, b: int) -> int:\n    return a + b\n\nfn sub(a: int, b: int) -> int:\n    return a - b\n\nfn mul(a: int, b: int) -> int:\n    return a * b\n\nfn neg(a: int) -> int:\n    return -a\n\nfn div(a: int, b: int) -> int:\n    return a // b\n\nfn power(a: int, b: int) -> int:\n    return a ** b\n\nfn shift(a: int, b: int) -> int:\n    return a << b\n\nfn total(xs: [int]) -> int:\n    return sum(xs)\n\nfn absolute(a: int) -> int:\n    return abs(a)\n\nfn grow(a: int) -> int:\n    var t = a\n    t += a\n    t *= 2\n    return t\n\nfn nested(a: int) -> int:\n    return (a * a) // a\n";

#[test]
fn every_int_operation_traps_outside_i64() {
    let script = "MAX, MIN = 2**63 - 1, -2**63\ncalls = [lambda: m.add(MAX, 1), lambda: m.sub(MIN, 1), lambda: m.mul(MAX, 2), lambda: m.neg(MIN), lambda: m.div(MIN, -1), lambda: m.power(2, 63), lambda: m.shift(1, 63), lambda: m.total([MAX, 1]), lambda: m.absolute(MIN), lambda: m.grow(MAX // 2 + 1), lambda: m.nested(2**32)]\nfor call in calls:\n    try:\n        print(call())\n    except Exception as e:\n        print(type(e).__name__)";
    assert_eq!(prints("overflow", ARITHMETIC, script), ["Overflow"; 11].join("\n"));
}

#[test]
fn int_operations_that_fit_give_python_s_results() {
    let script = "print(m.add(2, 3), m.sub(2, 3), m.mul(-4, 5), m.neg(7), m.div(-7, 2), m.power(3, 4), m.shift(1, 62), m.total([1, 2]), m.absolute(-3), m.grow(5), m.nested(7))";
    assert_eq!(prints("arithmetic", ARITHMETIC, script), "5 -1 -20 -7 -4 81 4611686018427387904 3 3 20 7");
}

#[test]
fn arithmetic_where_an_assignment_expression_cannot_go_still_traps() {
    let source =
        "fn f(n: int) -> [int]:\n    return [x for x in range(n * 2)]\n\nfn g(n: int = 3 * 3) -> int:\n    return n\n";
    let script = "print(len(m.f(3)), m.g())\ntry:\n    m.f(2**62)\nexcept Exception as e:\n    print(type(e).__name__)";
    assert_eq!(prints("no-walrus", source, script), "6 9\nOverflow");
}

#[test]
fn floats_do_not_trap_and_integer_division_gives_a_float() {
    let source = "fn big(x: f64) -> f64:\n    return x * 10.0\n\nfn half(n: int) -> f64:\n    return n / 2\n\nfn wrapped() -> int:\n    return wrapping_add(9223372036854775807, 1)\n";
    assert_eq!(prints("floats", source, "print(m.big(1e308), m.half(7), m.wrapped())"), "inf 3.5 -9223372036854775808");
}

// adr:0008: value semantics, copying only into and out of `var` and `inout` ------------------

#[test]
fn a_value_copied_into_a_var_is_its_own() {
    let source = "fn f() -> ([int], [int]):\n    xs = [1, 2]\n    var ys = xs\n    ys.append(3)\n    return (xs, ys)\n";
    assert_eq!(prints("into-var", source, "print(m.f())"), "([1, 2], [1, 2, 3])");
}

#[test]
fn a_var_read_into_another_binding_does_not_change_with_it() {
    let source = "fn f() -> ([int], [int]):\n    var xs = [1]\n    ys = xs\n    xs.append(2)\n    return (xs, ys)\n";
    assert_eq!(prints("out-of-var", source, "print(m.f())"), "([1, 2], [1])");
}

#[test]
fn a_var_stored_in_a_container_does_not_change_with_it() {
    let source = "fn f() -> [[int]]:\n    var row = [1]\n    var grid = [row]\n    row.append(2)\n    grid.append(row)\n    return grid\n";
    assert_eq!(prints("container", source, "print(m.f())"), "[[1], [1, 2]]");
}

#[test]
fn a_record_copied_into_a_var_is_its_own() {
    let source =
        "type P(x: int)\n\nfn f() -> (int, int):\n    a = P(1)\n    var b = a\n    b.x = 2\n    return (a.x, b.x)\n";
    assert_eq!(prints("record", source, "print(m.f())"), "(1, 2)");
}

#[test]
fn a_lambda_captures_a_copy() {
    let source = "fn f() -> int:\n    var n = 1\n    g = lambda: n\n    n = 2\n    return g()\n";
    assert_eq!(prints("capture", source, "print(m.f())"), "1");
}

#[test]
fn copies_are_made_only_where_a_var_or_an_inout_is_involved() {
    let none = compile("fn f(xs: [int]) -> int:\n    ys = xs\n    n = len(ys)\n    return n\n", Path::new("f.lotml"))
        .expect("compiles");
    assert!(!copies(&none), "{none}");
    let some = compile(
        "fn f(xs: [int]) -> int:\n    var ys = xs\n    ys.append(1)\n    return len(ys)\n",
        Path::new("f.lotml"),
    )
    .expect("compiles");
    assert!(copies(&some), "{some}");
}

// Parameter conventions -----------------------------------------------------------------------

#[test]
fn inout_arguments_are_written_back() {
    let source = "type P(x: int)\n\nfn reset(inout xs: [int]):\n    xs = [0]\n\nfn push(inout xs: [int], v: int):\n    xs.append(v)\n\nfn bump(inout n: int):\n    n += 1\n\nfn f() -> ([int], [int], int, int):\n    var a = [1]\n    reset(&a)\n    var b = [1]\n    push(&b, 2)\n    var p = P(1)\n    bump(&p.x)\n    var c = [5, 6]\n    bump(&c[1])\n    return (a, b, p.x, c[1])\n";
    assert_eq!(prints("inout", source, "print(m.f())"), "([0], [1, 2], 2, 7)");
}

#[test]
fn a_var_parameter_is_a_copy() {
    let source = "fn grow(var xs: [int]) -> int:\n    xs.append(1)\n    return len(xs)\n\nfn f() -> (int, int):\n    ys = [1]\n    n = grow(ys)\n    return (n, len(ys))\n";
    assert_eq!(prints("var-param", source, "print(m.f())"), "(2, 1)");
}

// adr:0002: errors as values ----------------------------------------------------------------

const PARSE: &str = "type ParseErr = Empty | NotNumber(text: str) | Negative(value: int)\n\nfn parse(s: str) -> int ! ParseErr:\n    if s == \"\":\n        fail Empty\n    n = s.to_int() ?? fail NotNumber(s)\n    if n < 0:\n        fail Negative(n)\n    return n\n\nfn double(s: str) -> int ! ParseErr:\n    n = parse(s)?\n    return n * 2\n\nfn describe(s: str) -> str:\n    match double(s):\n        case Ok(n):\n            return f\"number {n}\"\n        case Err(NotNumber(text)):\n            return f\"not a number: {text}\"\n        case Err(Negative(v)):\n            return f\"negative: {v}\"\n        case Err(_):\n            return \"empty\"\n";

#[test]
fn failures_are_values_passed_on_by_question_mark() {
    let script = "print(m.describe('21'), '|', m.describe('x'), '|', m.describe('-3'), '|', m.describe(''))";
    assert_eq!(prints("errors", PARSE, script), "number 42 | not a number: x | negative: -3 | empty");
}

#[test]
fn a_python_exception_at_the_boundary_is_an_error_value() {
    let script = "import lotml_rt\nr = lotml_rt.python(int, 'x')\nprint(type(r).__name__, r.error.kind)\nprint(lotml_rt.python(int, '7'))";
    assert_eq!(prints("boundary", "fn f() -> int:\n    return 1\n", script), "Err ValueError\nOk(value=7)");
}

// The prelude, the built-in methods, types and traits ----------------------------------------

#[test]
fn built_in_methods_follow_lotml() {
    let source = "fn f(s: str, d: {str: int}) -> str:\n    var xs = [3, 1, 2]\n    last = xs.pop() ?? 0\n    first = xs.find(lambda x: x > 1) ?? 0\n    at = s.find(\"z\") ?? -1\n    n = s.to_int() ?? 0\n    pair = s.split_once(\",\") ?? (\"\", \"\")\n    return f\"{last} {first} {at} {n} {pair} {len(d.keys())} {sorted(d.keys())} {d.get('q') ?? 9}\"\n";
    assert_eq!(prints("methods", source, "print(m.f('4,2', {'b': 1, 'a': 2}))"), "2 3 -1 0 ('4', '2') 2 ['a', 'b'] 9");
}

#[test]
fn keyword_arguments_of_built_in_methods_are_kept() {
    let source = "fn f(words: [str]) -> [str]:\n    var out = [w for w in words]\n    out.sort(key=lambda w: (len(w), w), reverse=True)\n    return out\n";
    assert_eq!(prints("keywords", source, "print(m.f(['bb', 'a', 'cc']))"), "['cc', 'bb', 'a']");
}

#[test]
fn sum_types_records_methods_and_traits() {
    let source = "trait Named:\n    fn name(self) -> str\n\n    fn greet(self) -> str:\n        return \"hi \" + self.name()\n\ntype Shape = Circle(r: f64) | Rect(w: f64, h: f64) | Empty\ntype Counter(count: int = 0)\n\nimpl Named for Counter:\n    fn name(self) -> str:\n        return f\"counter {self.count}\"\n\nimpl Counter:\n    fn bump(inout self):\n        self.count += 1\n\nfn area(s: Shape) -> f64:\n    match s:\n        case Circle(r):\n            return 3.0 * r * r\n        case Rect(w, h):\n            return w * h\n        case Empty:\n            return 0.0\n\nfn first[T](xs: [T]) -> T?:\n    return xs[0] if len(xs) > 0 else None\n\nfn f() -> str:\n    var c = Counter()\n    c.bump()\n    c.bump()\n    total = area(Circle(1.0)) + area(Rect(2.0, 3.0)) + area(Empty)\n    return f\"{total} {c.greet()} {first([7, 8]) ?? 0} {first([]) ?? 0}\"\n";
    assert_eq!(prints("types", source, "print(m.f())"), "9.0 hi counter 2 7 0");
}

#[test]
fn the_prelude_and_strings() {
    let source = "fn f(xs: [int]) -> str:\n    var h = Heap(xs)\n    h.push(0)\n    low = h.pop_min() ?? -1\n    pairs = list(zip(xs, reversed(xs)))\n    named = [f\"{i}:{x:>3}\" for i, x in enumerate(sorted(xs, key=lambda x: -x))]\n    return f\"{low} {len(h)} {pairs} {named} {max(xs)} {round(2.5)} {ord('a')} {chr(98)}\"\n";
    assert_eq!(
        prints("prelude", source, "print(m.f([3, 1, 2]))"),
        "0 3 [(3, 2), (1, 1), (2, 3)] ['0:  3', '1:  2', '2:  1'] 3 2 97 b"
    );
}

#[test]
fn todo_stops_the_program_where_it_is() {
    let out = run(
        "todo",
        "fn f(n: int) -> int:\n    if n > 0:\n        return todo()\n    return 0\n",
        "print(m.f(0))\nm.f(1)",
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).starts_with('0'));
    assert!(stderr.contains("prog.lotml\", line 3") && stderr.contains("Todo"), "{stderr}");
}

// R35: the prelude ----------------------------------------------------------------------------

#[test]
fn the_runtime_has_exactly_the_prelude_the_checker_knows() {
    let script = "import lotml_rt\nprint(' '.join(sorted(lotml_rt.PRELUDE)))";
    let runtime = prints("prelude-names", "fn f() -> int:\n    return 1\n", script);
    let mut checker: Vec<&str> = lotml_check::PRELUDE.to_vec();
    checker.sort_unstable();
    assert_eq!(runtime, checker.join(" "));
}

// The rest of the language, run end to end ----------------------------------------------------

const REST: &str = r#"type Point(x: int, y: int)
type Token = Num(int) | Word(str) | Stop

fn tokens(text: str) -> [Token]:
    var out = []
    for part in text.split():
        match part.to_int():
            case None:
                out.append(Stop if part == "." else Word(part))
            case n:
                out.append(Num(n))
    return out

fn describe(t: Token) -> str:
    match t:
        case Num(0):
            return "zero"
        case Num(n):
            return f"num {n!r}"
        case Word(w):
            return f"word {w:>4}"
        case Stop:
            return "stop"

fn shift(inout p: Point, by: int):
    p.x += by
    p.y -= by

fn rest() -> str:
    var p = Point(1, 2)
    shift(&p, 3)
    var grid = [[0, 0], [0, 0]]
    grid[1][0] += 5
    var i = 0
    while i < 3:
        i += 1
    squares = {k: k * k for k in range(4) if k != 2}
    odd = {x for x in [1, 2, 3] if x % 2 == 1}
    evens = [1, 2, 3, 4, 5, 6][1::2]
    pairs = sorted([(2, "b"), (1, "a")], reverse=True)
    total = sum(x for x in range(5))
    labels = [describe(t) for t in tokens("0 7 hi .")]
    found = 3 in odd and 9 not in odd and p is not None
    data = b"ab"
    nums = {"a": 1, "b": 2}
    keys = sorted(nums.keys())
    vals = sorted(nums.values())
    var h = Heap([5, 1, 3])
    h.push(0)
    low = h.pop_min() ?? -1
    word = "x" "y" + str(len(data))
    return f"{p.x},{p.y} {grid[1][0]} {i} {squares} {odd} {evens} {pairs[0]} {total} {labels} {found} {keys} {vals} {low} {word}"
"#;

#[test]
fn the_rest_of_the_language_runs() {
    assert_eq!(
        prints("rest", REST, "print(m.rest())"),
        "4,-1 5 3 {0: 0, 1: 1, 3: 9} {1, 3} [2, 4, 6] (2, 'b') 10 ['zero', 'num 7', 'word   hi', 'stop'] True ['a', 'b'] [1, 2] 0 xy2"
    );
}

#[test]
fn augmented_assignment_traps_on_fields_and_elements() {
    let source = "type C(n: int)\n\nfn grow(var c: C, var xs: [int]) -> (int, int):\n    c.n *= 4\n    xs[0] += c.n\n    return (c.n, xs[0])\n";
    let script = "print(m.grow(m.C(3), [1]))\ntry:\n    m.grow(m.C(2**62), [1])\nexcept Exception as e:\n    print(type(e).__name__)";
    assert_eq!(prints("augmented", source, script), "(12, 13)\nOverflow");
}

#[test]
fn the_emitter_never_lets_a_dunder_attribute_reach_python() {
    // The module JSON a `__` attribute would compile to is a call to the runtime, which raises,
    // not a live attribute access — defence in depth below the checker.
    let source = "fn f(n: int) -> int:\n    return n.__class__\n";
    // The checker refuses it, so `compile` returns an error.
    assert!(compile(source, std::path::Path::new("f.lotml")).is_err());
    // And the runtime has the guard the emitter routes such a name to.
    assert!(RUNTIME.contains("def forbidden("));
}
