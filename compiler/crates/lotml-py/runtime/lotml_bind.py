"""`lotml bind`: a Python module's stub read with Python's own parser, and its module-level
functions written as a lotml interface (adr:0012).

Run as `python -c <this file> <module> [<stub.pyi>]`; without a stub, typeshed's copy in an
installed mypy or jedi is used. Prints the interface on stdout. A function whose types lotml
cannot express is listed in a comment with the reason, never bound half-way.
"""

import ast
import importlib.util
import sys
from pathlib import Path

SIMPLE = {"int": "int", "float": "f64", "str": "str", "bool": "bool", "bytes": "bytes"}
# What a parameter accepts that a lotml value of this type satisfies.
ACCEPTS = {
    "SupportsIndex": "int",
    "SupportsInt": "int",
    "SupportsFloat": "f64",
    "StrPath": "str",
    "StrOrBytesPath": "str",
    "ReadableBuffer": "bytes",
}
SEQUENCES = {"list", "List"}
ABSTRACT_SEQUENCES = {"Sequence", "MutableSequence", "Iterable", "Collection"}
MAPPINGS = {"dict", "Dict"}
ABSTRACT_MAPPINGS = {"Mapping", "MutableMapping"}
SETS = {"set", "Set", "frozenset", "FrozenSet"}
ABSTRACT_SETS = {"AbstractSet", "MutableSet"}
TUPLES = {"tuple", "Tuple"}


class Unsupported(Exception):
    """A type lotml has no counterpart for."""


def name_of(node) -> str | None:
    if isinstance(node, ast.Name):
        return node.id
    if isinstance(node, ast.Attribute):
        return node.attr
    return None


def lotml_type(node, parameter: bool) -> str:
    """The lotml type for an annotation; a parameter may take an abstract collection, which a
    lotml list, dict or set satisfies, while a result must be the concrete one lotml receives."""
    if isinstance(node, ast.Constant) and node.value is None:
        return "None"
    if isinstance(node, ast.BinOp) and isinstance(node.op, ast.BitOr):
        sides = [node.left, node.right]
        rest = [s for s in sides if not (isinstance(s, ast.Constant) and s.value is None)]
        if len(rest) == 1:
            return optional(lotml_type(rest[0], parameter))
        raise Unsupported(f"`{ast.unparse(node)}` is a union, which lotml has no type for")
    name = name_of(node)
    if name is not None:
        if name in SIMPLE:
            return SIMPLE[name]
        if parameter and name in ACCEPTS:
            return ACCEPTS[name]
        raise Unsupported(f"`{ast.unparse(node)}` has no lotml type")
    if isinstance(node, ast.Subscript):
        head = name_of(node.value)
        args = node.slice.elts if isinstance(node.slice, ast.Tuple) else [node.slice]
        if head == "Optional" and len(args) == 1:
            return optional(lotml_type(args[0], parameter))
        if head in SEQUENCES or (parameter and head in ABSTRACT_SEQUENCES):
            return f"[{lotml_type(args[0], parameter)}]"
        if head in SETS or (parameter and head in ABSTRACT_SETS):
            return f"{{{lotml_type(args[0], parameter)}}}"
        if (head in MAPPINGS or (parameter and head in ABSTRACT_MAPPINGS)) and len(args) == 2:
            return f"{{{lotml_type(args[0], parameter)}: {lotml_type(args[1], parameter)}}}"
        if head in TUPLES:
            if any(isinstance(a, ast.Constant) and a.value is Ellipsis for a in args):
                raise Unsupported(f"`{ast.unparse(node)}` is a tuple of any length")
            return "(" + ", ".join(lotml_type(a, parameter) for a in args) + ")"
    raise Unsupported(f"`{ast.unparse(node)}` has no lotml type")


def optional(inner: str) -> str:
    return inner if inner.endswith("?") or inner == "None" else f"{inner}?"


def default(node) -> str:
    """A default lotml can write; any other is Python's, and the parameter is only optional."""
    if isinstance(node, ast.Constant) and not isinstance(node.value, (bytes, type(Ellipsis))):
        value = node.value
        if value is None:
            return "None"
        if isinstance(value, bool):
            return "True" if value else "False"
        if isinstance(value, (int, float)):
            return repr(value)
        if isinstance(value, str):
            return '"' + value.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n") + '"'
    return "todo()"


def signature(function: ast.FunctionDef) -> str:
    # `*args` and `**kwargs` take nothing when not given, so the named parameters are bound and
    # those are left to Python.
    arguments = function.args
    positional = arguments.posonlyargs + arguments.args
    defaults = [None] * (len(positional) - len(arguments.defaults)) + list(arguments.defaults)
    params = []
    for arg, given in [
        *zip(positional, defaults, strict=True),
        *zip(arguments.kwonlyargs, arguments.kw_defaults, strict=True),
    ]:
        if arg.annotation is None:
            raise Unsupported(f"`{arg.arg}` has no type")
        text = f"{arg.arg}: {lotml_type(arg.annotation, parameter=True)}"
        if given is not None:
            text += f" = {default(given)}"
        params.append(text)
    if function.returns is None:
        raise Unsupported("its result has no type")
    returns = lotml_type(function.returns, parameter=False)
    return f"fn {function.name}({', '.join(params)}) -> {returns} ! PyError"


def definitions(body: list) -> list:
    """The module-level functions, including those under `if sys.version_info …` blocks."""
    found = []
    for node in body:
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            found.append(node)
        elif isinstance(node, ast.If):
            found.extend(definitions(node.body) + definitions(node.orelse))
    return found


def interface(module: str, stub: Path) -> str:
    tree = ast.parse(stub.read_text(encoding="utf-8"), str(stub))
    bound, skipped, seen = [], [], set()
    overloaded = {
        f.name
        for f in definitions(tree.body)
        if any(name_of(d) == "overload" for d in f.decorator_list)
    }
    for function in definitions(tree.body):
        if function.name in seen or function.name.startswith("_"):
            continue
        seen.add(function.name)
        try:
            if function.name in overloaded:
                raise Unsupported("it is overloaded")
            if isinstance(function, ast.AsyncFunctionDef):
                raise Unsupported("it is a coroutine")
            bound.append(signature(function))
        except Unsupported as why:
            skipped.append(f"#   {function.name}: {why}")
    lines = [
        f"# The Python module `{module}`, bound by `lotml bind` from {stub.name}.",
        "# Do not edit: run `lotml bind` again.",
        "# Every function returns `T ! PyError`: a stub does not say what a call raises.",
        "# A parameter written `= todo()` is optional: Python supplies its default.",
        *bound,
    ]
    if skipped:
        lines += ["", "# Not bound, as lotml has no type for them:", *skipped]
    return "\n".join(lines) + "\n"


def typeshed(module: str) -> Path | None:
    """typeshed's stub for a standard-library module, from an installed mypy or jedi."""
    parts = module.split(".")
    for package, inner in (("mypy", "typeshed/stdlib"), ("jedi", "third_party/typeshed/stdlib")):
        spec = importlib.util.find_spec(package)
        if spec is None or spec.origin is None:
            continue
        root = Path(spec.origin).parent / inner
        for candidate in (
            root.joinpath(*parts).with_suffix(".pyi"),
            root.joinpath(*parts, "__init__.pyi"),
        ):
            if candidate.is_file():
                return candidate
    return None


def main(args: list[str]) -> int:
    module = args[0]
    stub = Path(args[1]) if len(args) > 1 and args[1] else typeshed(module)
    if stub is None:
        sys.stderr.write(f"no stub for `{module}`: give one with --stub <file.pyi>\n")
        return 2
    try:
        sys.stdout.write(interface(module, stub))
    except (OSError, SyntaxError, UnicodeDecodeError) as error:
        sys.stderr.write(f"cannot read {stub}: {error}\n")
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
