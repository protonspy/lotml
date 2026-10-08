//! lotml and Python calling each other (R14, R27; adr:0012): `lotml bind` writes a Python
//! module's interface from its stub, a lotml program calls it with every call `T ! PyError`, and
//! Python calls a compiled module through a checked boundary.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const TEXTWRAP_PYI: &str = "\
from collections.abc import Callable

class TextWrapper:
    width: int

def wrap(text: str, width: int = 70, *, max_lines: int | None = None, placeholder: str = \" [...]\") -> list[str]: ...
def fill(text: str, width: int = 70, **kwargs) -> str: ...
def shorten(text: str, width: int, *, placeholder: str = ...) -> str: ...
def dedent(text: str) -> str: ...
def indent(text: str, prefix: str, predicate: Callable[[str], bool] | None = None) -> str: ...
@overload
def pick(x: int) -> int: ...
@overload
def pick(x: str) -> str: ...
def _private(x: int) -> int: ...
";

fn lotml(args: &[&str], dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lotml")).args(args).current_dir(dir).output().expect("the binary runs")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n")
}

fn scratch(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("interop").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    for (file, text) in files {
        let path = dir.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).expect("a scratch file");
    }
    dir
}

fn python(dir: &Path, script: &str) -> Output {
    let python = lotml_py::python().expect("a Python interpreter: python3, python, py -3 or LOTML_PYTHON");
    Command::new(&python[0])
        .args(&python[1..])
        .arg("-c")
        .arg(script)
        .current_dir(dir)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .output()
        .expect("Python runs")
}

#[test]
fn bind_writes_an_interface_from_a_stub() {
    let dir = scratch("bind", &[("stubs/textwrap.pyi", TEXTWRAP_PYI)]);
    let out = lotml(&["bind", "textwrap", "--stub", "stubs/textwrap.pyi"], &dir);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(stdout(&out).contains("4 functions bound, 2 not"), "{}", stdout(&out));
    let interface = std::fs::read_to_string(dir.join("bindings").join("py.textwrap.lotmli")).unwrap();
    assert!(interface.contains(
        "fn wrap(text: str, width: int = 70, max_lines: int? = None, placeholder: str = \" [...]\") -> [str] ! PyError\n"
    ));
    assert!(interface.contains("fn fill(text: str, width: int = 70) -> str ! PyError\n"), "**kwargs is left to Python");
    assert!(interface.contains("fn shorten(text: str, width: int, placeholder: str = todo()) -> str ! PyError\n"));
    assert!(interface.contains("#   indent: `Callable[[str], bool]` has no lotml type\n"), "{interface}");
    assert!(interface.contains("#   pick: it is overloaded"));
    assert!(!interface.contains("_private") && !interface.contains("TextWrapper"));
}

/// A stub written as typeshed writes `random`: module-level names bound to the methods of an
/// instance it declares, some under a version check, one of a method the stub does not hold.
const ALIASES_PYI: &str = "\
import sys

class Random(_random.Random):
    def randint(self, a: int, b: int) -> int: ...
    if sys.version_info >= (3, 12):
        def binomialvariate(self, n: int = 1, p: float = 0.5) -> int: ...
    if sys.version_info >= (3, 11):
        def triangular(self, low: float = 0.0, high: float = 1.0) -> float: ...
    else:
        @overload
        def triangular(self, low: float) -> float: ...
        @overload
        def triangular(self, low: str) -> str: ...
    @overload
    def pick(self, x: int) -> int: ...
    @overload
    def pick(self, x: str) -> str: ...
    @staticmethod
    def make(seed: int) -> float: ...
    @classmethod
    def named(cls, name: str) -> str: ...

_inst: Random
randint = _inst.randint
if sys.version_info >= (3, 12):
    binomialvariate = _inst.binomialvariate
triangular = _inst.triangular
pick = _inst.pick
make = _inst.make
named = _inst.named
getrandbits = _inst.getrandbits
_hidden = _inst.randint
";

#[test]
fn bind_binds_the_names_a_stub_writes_as_methods_of_an_instance() {
    let dir = scratch("bind-aliases", &[("stubs/rand.pyi", ALIASES_PYI)]);
    let out = lotml(&["bind", "py.rand", "--stub", "stubs/rand.pyi"], &dir);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(!dir.join("bindings").join("py.py.rand.lotmli").exists(), "the origin is given once");
    let interface = std::fs::read_to_string(dir.join("bindings").join("py.rand.lotmli")).unwrap();
    for bound in [
        "fn randint(a: int, b: int) -> int ! PyError\n",
        "fn binomialvariate(n: int = 1, p: f64 = 0.5) -> int ! PyError\n",
        "fn triangular(low: f64 = 0.0, high: f64 = 1.0) -> f64 ! PyError\n",
        "fn make(seed: int) -> f64 ! PyError\n",
        "fn named(name: str) -> str ! PyError\n",
    ] {
        assert!(interface.contains(bound), "{bound}in\n{interface}");
    }
    assert!(interface.contains("#   pick: it is overloaded\n"), "{interface}");
    assert!(interface.contains("#   getrandbits: `Random` holds no `getrandbits` in this stub\n"), "{interface}");
    assert!(!interface.contains("_hidden"));
    assert!(stdout(&out).contains("5 functions bound, 2 not"), "{}", stdout(&out));
}

#[test]
fn bind_refuses_a_name_that_is_not_a_module() {
    let dir = scratch("bind-name", &[]);
    let out = lotml(&["bind", "../escape", "--stub", "x.pyi"], &dir);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("is not a Python module name"));
    assert!(!dir.join("bindings").exists());
}

const LIAR_PY: &str =
    "def count() -> int:\n    return 'many'\n\ndef boom(n: int) -> int:\n    raise ValueError(f'no {n}')\n";
const LIAR_PYI: &str = "def count() -> int: ...\ndef boom(n: int) -> int: ...\n";

#[test]
fn a_lotml_program_calls_python_through_its_interface() {
    let program = "\
from py.textwrap import dedent, wrap
import py.liar

fn main() -> None ! PyError:
    for line in wrap(dedent(\"    one two three four\")?, width=8)?:
        print(line)
    match py.liar.count():
        case Ok(n):
            print(n)
        case Err(e):
            print(e.kind)
    match py.liar.boom(3):
        case Ok(n):
            print(n)
        case Err(e):
            print(f\"{e.kind}: {e.message}\")
";
    let dir = scratch(
        "call-python",
        &[
            ("stubs/textwrap.pyi", TEXTWRAP_PYI),
            ("stubs/liar.pyi", LIAR_PYI),
            ("liar.py", LIAR_PY),
            ("app/main.lotml", program),
        ],
    );
    assert!(lotml(&["bind", "textwrap", "--stub", "stubs/textwrap.pyi"], &dir).status.success());
    assert!(lotml(&["bind", "liar", "--stub", "stubs/liar.pyi"], &dir).status.success());
    // The bindings sit above the program's directory, and are found from there.
    let checked = lotml(&["check", "app/main.lotml"], &dir);
    assert!(checked.status.success(), "{}", stdout(&checked));
    let out = lotml(&["run", "app/main.lotml"], &dir);
    assert_eq!(
        stdout(&out),
        "one two\nthree\nfour\nTypeError\nValueError: no 3\n",
        "a value of the wrong type is an error, not a wrong value: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A Python module whose values no stub types: an object, a dataclass, and functions that change
/// what they are given.
const OBJS_PY: &str = "\
import dataclasses

class Box:
    def __init__(self):
        self.n = 1

@dataclasses.dataclass
class Data:
    n: int

def make(): return Box()
def size(o): return o.n
def bump(o): o.n += 1
def data(): return Data(10)
def touch(d): d.n += 5
def peek(d): return d.n
def numbers(): return [1, 2, 3]
def word(): return 'x'
def send(xs):
    xs.append(99)
    return len(xs)
";

const OBJS_LOTMLI: &str = "\
fn make() -> PyObject ! PyError
fn size(o: PyObject) -> int ! PyError
fn bump(o: PyObject) -> None ! PyError
fn data() -> PyObject ! PyError
fn touch(d: PyObject) -> None ! PyError
fn peek(d: PyObject) -> int ! PyError
fn numbers() -> PyObject ! PyError
fn word() -> PyObject ! PyError
fn send(xs: PyObject) -> int ! PyError
";

#[test]
fn a_python_object_crosses_as_the_object_it_is_and_leaves_through_a_checked_conversion() {
    let program = "\
import py.objs

fn as_int(o: PyObject) -> int ! PyError:
    n: int = o.value()?
    return n

fn main() -> None ! PyError:
    b = py.objs.make()?
    py.objs.bump(b)?
    print(py.objs.size(b)?)
    d = py.objs.data()?
    py.objs.touch(d)?
    print(py.objs.peek(d)?)
    xs: [int] = py.objs.numbers()?.value()?
    print(xs)
    var mine = [1, 2]
    print(py.objs.send(mine)?, mine)
    match as_int(py.objs.word()?):
        case Ok(n):
            print(n)
        case Err(e):
            print(e.kind)
";
    let dir = scratch(
        "python-object",
        &[("objs.py", OBJS_PY), ("bindings/py.objs.lotmli", OBJS_LOTMLI), ("main.lot", program)],
    );
    let checked = lotml(&["check", "main.lot"], &dir);
    assert!(checked.status.success(), "{}", stdout(&checked));
    let out = lotml(&["run", "main.lot"], &dir);
    assert_eq!(
        stdout(&out),
        "2\n15\n[1, 2, 3]\n3 [1, 2]\nTypeError\n",
        "the object and the dataclass changed in Python, the LotML list copied: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn a_broken_interface_is_reported_by_check() {
    let dir = scratch(
        "broken-interface",
        &[("bindings/m.lotmli", "fn f(x: int) -> int\n"), ("a.lotml", "fn g() -> int:\n    return 1\n")],
    );
    let out = lotml(&["check", "a.lotml"], &dir);
    assert_eq!(out.status.code(), Some(1));
    assert!(stdout(&out).contains("m.lotmli") && stdout(&out).contains("E0221"), "{}", stdout(&out));
}

const LIBRARY: &str = "\
type Shape = Circle(r: f64) | Empty
type ParseErr = Bad(text: str)

fn area(s: Shape) -> f64:
    match s:
        case Circle(r):
            return 3.0 * r * r
        case Empty:
            return 0.0

fn parse(s: str) -> int ! ParseErr:
    return s.to_int() ?? fail Bad(s)

fn grow(xs: [int], by: int = 1) -> [int]:
    var ys = xs
    ys.append(by)
    return ys
";

#[test]
fn python_calls_a_compiled_module_through_a_checked_boundary() {
    let dir = scratch("python-calls", &[("library.lotml", LIBRARY)]);
    let built = lotml(&["build", "--target", "python", "library.lotml", "-o", "out"], &dir);
    assert!(built.status.success(), "{}", stdout(&built));
    let stub = std::fs::read_to_string(dir.join("out").join("library_lotml.pyi")).unwrap();
    assert!(stub.contains("def area(s: Shape) -> float: ..."));
    assert!(stub.contains("Shape: TypeAlias = Circle | lotml_rt.Unit"));
    assert!(stub.contains(
        "# Raises lotml_rt.LotmlError when it fails; its `error` is a ParseErr.\ndef parse(s: str) -> int: ..."
    ));
    let script = "\
import sys
sys.path.insert(0, 'out')
import library_lotml as m, lotml_rt
print(m.area(m.Circle(2)), m.area(m.Empty))
print(m.parse('12'))
try:
    m.parse('x')
except lotml_rt.LotmlError as e:
    print('error', e.error)
for call in (lambda: m.area('x'), lambda: m.grow([1, 'a']), lambda: m.grow([2**64]), lambda: m.grow(xs=[1], extra=2)):
    try:
        call()
    except (TypeError, OverflowError) as e:
        print(type(e).__name__, e)
xs = [1]
print(m.grow(xs), m.grow(xs, by=5), xs)
";
    let out = python(&dir, script);
    assert_eq!(
        stdout(&out),
        "12.0 0.0\n12\nerror Bad(text='x')\n\
         TypeError area(s): expected Shape, got str\n\
         TypeError grow(xs)[1]: expected int, got str\n\
         OverflowError grow(xs)[0]: 18446744073709551616 does not fit in int\n\
         TypeError grow() has no parameter `extra`\n\
         [1, 1] [1, 5] [1]\n",
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn run_and_test_still_see_the_program_unwrapped() {
    let program = "type E = Bad\n\nfn main() -> None ! E:\n    fail Bad\n\ntest \"t\":\n    assert 1 == 1\n";
    let dir = scratch("unwrapped", &[("p.lotml", program)]);
    let out = lotml(&["run", "p.lotml"], &dir);
    assert_eq!(out.status.code(), Some(1), "a failing main is an error, not a panic");
    let tested = lotml(&["test", "p.lotml"], &dir);
    assert!(tested.status.success(), "{}", stdout(&tested));
}

/// `-P`: the working directory is searched after the library, so a planted `json.py` is never
/// imported by the scripts `lotml test`, `lotml run` and `lotml bind` start.
#[test]
fn a_python_module_left_in_the_directory_never_shadows_the_library() {
    let planted = "import sys\nsys.stdout.write('PLANTED')\nsys.exit(3)\n";
    let program = "fn main():\n    print(1)\n\ntest \"t\":\n    assert 1 == 1\n";
    let dir = scratch(
        "planted",
        &[
            ("json.py", planted),
            ("ast.py", planted),
            ("importlib.py", planted),
            ("p.lotml", program),
            ("stub.pyi", "def f(x: int) -> int: ...\n"),
        ],
    );
    for args in [vec!["test", "p.lotml"], vec!["run", "p.lotml"], vec!["bind", "stubbed", "--stub", "stub.pyi"]] {
        let out = lotml(&args, &dir);
        assert!(out.status.success() && !stdout(&out).contains("PLANTED"), "{args:?}: {}", stdout(&out));
    }
}

#[test]
fn bind_refuses_a_device_s_name() {
    let dir = scratch("bind-device", &[]);
    for name in ["nul", "con", "com1", "lpt9.x"] {
        let out = lotml(&["bind", name, "--stub", "x.pyi"], &dir);
        assert_eq!(out.status.code(), Some(2), "{name}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("device"), "{name}");
    }
    let out = lotml(&["bind", "com\u{b9}", "--stub", "x.pyi"], &dir);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("not a Python module name"));
}
