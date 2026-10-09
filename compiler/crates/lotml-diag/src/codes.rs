//! Every diagnostic code, its title and its explanation page. A code is never reused: a
//! diagnostic that changes meaning gets a new one and the old stays, retired.

pub struct Code {
    pub code: &'static str,
    pub title: &'static str,
    pub explanation: &'static str,
}

pub fn find(code: &str) -> Option<&'static Code> {
    ALL.iter().find(|c| c.code.eq_ignore_ascii_case(code))
}

pub const ALL: &[Code] = &[
    Code {
        code: "E0001",
        title: "unreadable text",
        explanation: "A character no token starts with, or a string that is never closed.\n\n```\nx = \"abc\n```\n\nClose the string on its line, or use a triple-quoted string for text that spans lines.",
    },
    Code {
        code: "E0002",
        title: "indentation that matches no block",
        explanation: "Blocks are indented by four spaces per level. A line indented deeper than its block, \
        or dedented to a column no enclosing block starts at, is reported with the indentation that fits.\n\n\
        ```\nfn f():\n    if x:\n        y = 1\n      z = 2   # matches neither block\n```",
    },
    Code {
        code: "E0003",
        title: "unexpected token",
        explanation: "The parser expected something else here; the message names what it expected and what it found. \
        A missing `:` or closing bracket comes with the fix.",
    },
    Code {
        code: "E0004",
        title: "malformed string or f-string",
        explanation: "An escape that is not valid (`\\xZZ`), or an f-string field that is empty or never closed. \
        Write a literal brace in an f-string as `{{` or `}}`.",
    },
    Code {
        code: "E0101",
        title: "`def` instead of `fn`",
        explanation: "lotml declares functions with `fn`, and every parameter and the return type are annotated:\n\n\
        ```\nfn add(a: int, b: int) -> int:\n    return a + b\n```",
    },
    Code {
        code: "E0102",
        title: "a class",
        explanation: "lotml has no classes or inheritance. Declare the data as a record and the methods in `impl`:\n\n\
        ```\ntype Counter(count: int)\n\nimpl Counter:\n    fn bump(inout self):\n        self.count += 1\n```",
    },
    Code {
        code: "E0103",
        title: "a brace block",
        explanation: "A block starts with `:` at the end of its header line and is indented on the next lines; \
        braces make dicts and sets, not blocks.",
    },
    Code {
        code: "E0104",
        title: "`import *`",
        explanation: "Name what you import: `from math import sqrt, pi`. The prelude already holds the common names.",
    },
    Code {
        code: "E0105",
        title: "`else if` instead of `elif`",
        explanation: "A chain of conditions is `if … elif … else`, as in Python. The fix rewrites `else if` as `elif`.",
    },
    Code {
        code: "E0106",
        title: "a match arm without `case`",
        explanation: "Each arm of a `match` starts with `case`, a pattern and `:`:\n\n\
        ```\nmatch shape:\n    case Circle(r):\n        return 3.14 * r * r\n```",
    },
    Code {
        code: "E0107",
        title: "`raise`",
        explanation: "lotml has no exceptions. A function that can fail declares it in its type and returns the error \
        with `fail`:\n\n```\nfn parse(s: str) -> int ! ParseErr:\n    if s == \"\":\n        fail Empty\n```",
    },
    Code {
        code: "E0108",
        title: "`try` and `except`",
        explanation: "lotml has no exceptions. Match a result with `case Ok(v)` and `case Err(e)`, unwrap it with `??`, \
        or pass its error on with `?`.",
    },
    Code {
        code: "E0109",
        title: "a Python statement lotml does not have",
        explanation: "`with`, `yield`, `global` and `nonlocal` have no lotml equivalent. Return a list instead of \
        yielding; pass values in and out instead of sharing globals.",
    },
    Code {
        code: "E0110",
        title: "`let` or `const`",
        explanation: "A local is declared by assigning it, and is immutable: `n = len(xs)`. One that changes is \
        declared with `var`: `var total = 0`. The fix drops `let` and `const`, and writes `let mut` as `var`.",
    },
    Code {
        code: "E0111",
        title: "an operator from another language",
        explanation: "lotml writes the boolean operators as words — `and`, `or`, `not` — and has no `++` or `--`: \
        write `x += 1`. The fix rewrites each.",
    },
    Code {
        code: "E0112",
        title: "`async` or `await`",
        explanation: "lotml's functions have no colour: none is marked `async`, and none is awaited. A call that \
        waits blocks only its own task; run tasks at once with `parallel`, which waits for them all:\n\n\
        ```\npages = parallel([lambda: fetch(a), lambda: fetch(b)])\n```\n\nThe fix removes the keyword.",
    },
    Code {
        code: "E0201",
        title: "an unresolved name",
        explanation: "No variable, function, type or variant of this name is in scope. The diagnostic lists the names \
        in scope that are closest; a name used before its declaration in a function body is not yet in scope.",
    },
    Code {
        code: "E0202",
        title: "an unknown type",
        explanation: "Types are the primitives (`int`, `f64`, `str`, `bool`, …), `[T]`, `{K: V}`, `{T}`, tuples, \
        `T?`, records and sum types declared with `type`, and type parameters in brackets.",
    },
    Code {
        code: "E0203",
        title: "the wrong number of arguments",
        explanation: "A call passes a different number of arguments from the parameters the function declares, \
        counting the ones with defaults.",
    },
    Code {
        code: "E0204",
        title: "mismatched types",
        explanation: "A value of one type where another is expected. The diagnostic shows both; lotml converts no \
        type implicitly except `int` operands of `/`, so `1 + 2.0` needs `float(1) + 2.0`.",
    },
    Code {
        code: "E0205",
        title: "an unknown field or method",
        explanation: "The type has no field or method of this name. The diagnostic lists the ones it has.",
    },
    Code {
        code: "E0206",
        title: "a match that misses cases",
        explanation: "A `match` must cover every variant of a sum type (or end with `case _`). The diagnostic lists \
        the variants no arm covers, and the fix adds an arm for each.",
    },
    Code {
        code: "E0207",
        title: "an optional used as its value",
        explanation: "A `T?` may be `None`. Test it with `if x is not None:` before using it as a `T`, give a \
        default with `x ?? default`, or fail with `x ?? fail error`.",
    },
    Code {
        code: "E0208",
        title: "a condition that is not `bool`",
        explanation: "`if`, `while`, `and`, `or` and `not` take a `bool`: lotml has no truthiness. Write \
        `len(xs) > 0`, `s != \"\"`, `n != 0` or `x is not None`; the fix writes the one for the value's type.",
    },
    Code {
        code: "E0209",
        title: "a function that may end without returning",
        explanation: "A function with a return type must return a value on every path. Add a `return` at the end, \
        or `todo()` for a path not written yet.",
    },
    Code {
        code: "E0210",
        title: "a name declared twice",
        explanation: "Two items, fields, variants or parameters share a name in the same scope.",
    },
    Code {
        code: "E0211",
        title: "a pattern of the wrong shape",
        explanation: "A pattern names a variant the matched type does not have, or gives it the wrong number of \
        fields.",
    },
    Code {
        code: "E0212",
        title: "something called that is not a function",
        explanation: "Only functions, methods, records, variants and lambdas can be called.",
    },
    Code {
        code: "E0213",
        title: "a type that does not implement a trait",
        explanation: "A generic parameter bounded by a trait was given a type that does not implement it.",
    },
    Code {
        code: "E0214",
        title: "`?` or `fail` where an error cannot go",
        explanation: "`expr?` and `fail e` return an error from the current function, so the function must declare \
        it: `-> T ! E`, with the same `E`. In a test, `expr?` fails the test.",
    },
    Code {
        code: "E0215",
        title: "`??` on a value that is not optional",
        explanation: "`x ?? default` gives the value inside a `T?`. On any other type it has nothing to do.",
    },
    Code {
        code: "E0216",
        title: "an unknown module or import",
        explanation: "lotml programs import from the standard modules (`math`); the common names need no import. \
        A Python module is imported by its origin, `import py.textwrap` or `from py.textwrap import dedent`, \
        through the interface the compiler generates from its stub on import, or `bindings/py.textwrap.lotmli` \
        where one applies; `lotml bind textwrap` says why a module has none. A C library is imported as \
        `c.<library>`. A bare `import textwrap` names a LotML module only.",
    },
    Code {
        code: "E0217",
        title: "a parameter without a type",
        explanation: "Every parameter of a function is annotated, except `self` in a method.",
    },
    Code {
        code: "E0218",
        title: "a value returned where none is expected, or none where one is",
        explanation: "A function without `-> T` returns `None`; `return value` there is an error, and so is a bare \
        `return` in a function that declares a type.",
    },
    Code {
        code: "E0219",
        title: "a result used as its value, or dropped",
        explanation: "A call to a function declared `-> T ! E` gives a result, not a `T`. Unwrap it with `?` in a \
        function that fails with the same `E`, or `match` on `Ok(v)` and `Err(e)`. A statement that drops a \
        result drops its error, which is never what a failure is for.",
    },
    Code {
        code: "E0220",
        title: "a name reserved for the compiler",
        explanation: "A name starting with `__` is the compiler's: it marks the runtime, the test list and \
        the generated helpers a program must not reach or shadow. Name your own values without the leading `__`.",
    },
    Code {
        code: "E0221",
        title: "not allowed in an interface",
        explanation: "An interface (`bindings/py.<module>.lotmli`) declares the functions of a Python module, as \
        `lotml bind` wrote them: one signature per line, no body, no type parameters, over the types every \
        program has, each returning `T ! PyError` because any call into Python can fail:\n\n    \
        fn dedent(text: str) -> str ! PyError\n\nRegenerate the file with `lotml bind <module> --stub <file.pyi>` \
        rather than editing it.",
    },
    Code {
        code: "E0222",
        title: "a type too large to check",
        explanation: "The type of an expression has more than 1,024 parts, or the types of a function's or a \
        module's expressions have too many in all. A type is built from its parts, so a tuple paired with \
        itself doubles it at each step:\n\n    t1 = (t0, t0)\n    t2 = (t1, t1)\n\nand thirty such lines ask \
        for a type of a billion parts. Name the shape with a record type, or keep the values in a list, whose \
        type is one part however long it grows.",
    },
    Code {
        code: "E0223",
        title: "a format spec the value refuses",
        explanation: "The spec after `:` in an f-string field follows Python's format mini-language, \
        `[[fill]align][sign][z][#][0][width][grouping][.precision][type]`, and what it may hold depends on the \
        value: text takes no sign and no `d`, an integer takes no precision, a float no `x`, and a list, a \
        dict or a record takes no spec at all.\n\n    f\"{name:>10}\"     # text, right-aligned\n    \
        f\"{count:,d}\"     # an integer, with thousands separated\n    f\"{ratio:.2%}\"    # a float, as a \
        percentage\n\nA width or precision is at most 10000, and a spec holds no `{field}`: compute the width \
        into the text, or pad with `ljust`, `rjust` or `center`.",
    },
    Code {
        code: "E0224",
        title: "a bindings file shadowing a generated interface",
        explanation: "The compiler generates a Python module's interface from its stub when a program imports it. \
        A `bindings/py.<module>.lotmli` that applies to the import is used instead, so a stub's later versions no \
        longer reach the program. Delete the file to use the generated interface, or keep it on purpose to type \
        the module by hand.",
    },
    Code {
        code: "E0225",
        title: "a stub that differs from the lock",
        explanation: "`lotml.lock` records the SHA-256 of the stub each `py.` module was bound from. When the stub \
        found on import differs, the compiler binds from it, so the program checks against what it will run with, \
        and warns: the project's environment changed since the lock was written. `lotml bind --lock` records the \
        stubs found; `lotml check --locked` fails on the difference, for CI.",
    },
    Code {
        code: "E0301",
        title: "an immutable reassigned",
        explanation: "A local declared with `x = …` cannot be assigned again. Declare it `var x = …` if it changes:\n\n\
        ```\nvar total = 0\nfor x in xs:\n    total += x\n```",
    },
    Code {
        code: "E0302",
        title: "an immutable mutated",
        explanation: "Appending to a list, setting a field or an element, or calling an `inout self` method needs a \
        `var`. A parameter is read-only: to change the caller's value, declare it `inout` and pass `&x`; to change \
        a copy, write `var mine = param`.",
    },
    Code {
        code: "E0303",
        title: "an `inout` argument that is not a `var`",
        explanation: "`&x` lends `x` to a function that changes it, so `x` must be a `var` (or an `inout` \
        parameter itself).",
    },
    Code {
        code: "E0304",
        title: "an `inout` parameter passed without `&`",
        explanation: "A function that changes its argument says so at the call: `add_all(&xs, values)`. The fix \
        adds the `&`.",
    },
    Code {
        code: "E0305",
        title: "`&` for a parameter that is not `inout`",
        explanation: "Only an `inout` parameter takes `&x`; any other gets a copy.",
    },
    Code {
        code: "E0306",
        title: "a value used after it was given away",
        explanation: "A `sink` parameter takes its argument over; the caller may not use the variable after the call.",
    },
    Code {
        code: "E0307",
        title: "one place passed as `inout` twice",
        explanation: "Two `inout` arguments of one call may not be the same variable, field or element.",
    },
    Code {
        code: "E0308",
        title: "a parameter changed and then dropped",
        explanation: "A function copies its parameter into a `var` and changes it, but the change never leaves the \
        function. If the caller should see it, declare the parameter `inout`; in Python the change would have \
        reached the caller, in lotml it does not.",
    },
    Code {
        code: "E0401",
        title: "a Python module in a native program",
        explanation: "A program built natively (`--target llvm`) runs without Python, so it cannot import a \
        Python module through its interface. Run it with `lotml run`, on CPython, where every Python library is \
        reachable; or, to build it natively, write the function in LotML or import it from a C library's \
        `c.<library>` interface.",
    },
    Code {
        code: "E0402",
        title: "not compiled by the LLVM backend yet",
        explanation: "The LLVM backend does not compile this construct yet. The program checks and runs on the \
        Python target: run it with `lotml run`, or build it with `--target python`.",
    },
    Code {
        code: "E0403",
        title: "a function left out of a C library",
        explanation: "`lotml build --shared` exports the top-level functions whose parameters and result C can be \
        given without a rule for who frees them: integers, `f32`, `f64`, `bool`, `None` as the result, and `str` \
        as a parameter. A function taking or returning anything else — a list, a record, a result, an `inout` \
        parameter — or a generic one is left out, and this warning names what excluded it. To export it, give it \
        a signature of those types, perhaps as a small function calling it.",
    },
    Code {
        code: "E0404",
        title: "a C library with nothing to export",
        explanation: "`lotml build --shared` found no top-level function, other than `main`, that C can call. \
        The E0403 warnings beside this error name what left each function out.",
    },
    Code {
        code: "E0405",
        title: "a C library named like the runtime",
        explanation: "A library's functions are exported as `<module>_<function>`, `<module>` the file's name. A \
        library whose names come out starting `lt_` would export them under the names the runtime it carries \
        gives its own functions, and the two would clash when linked. Name the file otherwise.",
    },
];
