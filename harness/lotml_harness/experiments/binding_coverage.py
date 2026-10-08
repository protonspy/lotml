"""How much of Python a LotML program can call typed (specs/binding-coverage): for each module of
a fixed corpus, the public names its stub declares and those `lotml bind` binds from it.

The corpus is `binding-corpus.json`; every stub is read from a distribution of the harness group
`stubs`, pinned in the lock (adr:0030), and no module of the corpus is imported. Each measurement
is kept under a label in `results/binding-coverage/<label>.json`, and the report
`results/binding-coverage.md` shows every label recorded, so the share before a change to the
binder stays beside the share after it.

    uv sync --group stubs
    python -m lotml_harness.experiments.binding_coverage <label> [--binder <lotml_bind.py>]
"""

import argparse
import ast
import hashlib
import importlib.metadata
import importlib.util
import json
import sysconfig
from dataclasses import dataclass, field
from datetime import UTC, datetime
from pathlib import Path
from types import ModuleType

from lotml_harness import ROOT
from lotml_harness.experiments.build_speed import LABEL
from lotml_harness.experiments.phase1 import RESULTS

CORPUS = ROOT / "harness" / "binding-corpus.json"
BINDER = ROOT / "compiler" / "crates" / "lotml-py" / "runtime" / "lotml_bind.py"
RECORDS = RESULTS / "binding-coverage"
REPORT = RESULTS / "binding-coverage.md"
GROUPS = {"stdlib": "the standard library", "pypi": "PyPI"}


@dataclass
class Module:
    name: str
    group: str
    """`stdlib` or `pypi`."""
    distribution: str
    stub: str | None = None
    """The stub read, relative to the root it was found under; `None` when there is none."""
    public: int = 0
    bound: list[str] = field(default_factory=list)
    """The public names `lotml bind` binds typed."""

    @property
    def share(self) -> float:
        return len(self.bound) / self.public if self.public else 0.0


@dataclass
class Record:
    label: str
    modules: list[Module]
    versions: dict[str, str]
    """The version of each distribution read."""
    corpus: str
    """A digest of the corpus measured."""
    taken: str = ""

    def share(self, group: str | None = None) -> float:
        chosen = [m for m in self.modules if group is None or m.group == group]
        public = sum(m.public for m in chosen)
        return sum(len(m.bound) for m in chosen) / public if public else 0.0


def corpus(text: str) -> list[Module]:
    """The modules `binding-corpus.json` names, standard library first."""
    data = json.loads(text)
    stdlib = data["stdlib"]
    modules = [Module(name, "stdlib", stdlib["distribution"]) for name in stdlib["modules"]]
    modules += [Module(entry["module"], "pypi", entry["distribution"]) for entry in data["pypi"]]
    return modules


def typeshed_root() -> Path | None:
    """typeshed's `stdlib` in the installed mypy, found without importing mypy."""
    spec = importlib.util.find_spec("mypy")
    if spec is None or spec.origin is None:
        return None
    return Path(spec.origin).parent / "typeshed" / "stdlib"


def stub_of(module: Module, typeshed: Path | None, purelib: Path) -> Path | None:
    """The stub of `module`: typeshed's for the standard library; for a PyPI module, in the order
    PEP 561 gives, its stub distribution's, a `.pyi` beside it, or its own source when it is
    marked `py.typed`."""
    parts = module.name.split(".")
    if module.group == "stdlib":
        roots = [(typeshed, ".pyi")] if typeshed is not None else []
    else:
        top, rest = parts[0], parts[1:]
        typed = (purelib / top / "py.typed").is_file()
        roots = [(purelib / f"{top}-stubs", ".pyi"), (purelib / top, ".pyi")]
        roots += [(purelib / top, ".py")] if typed else []
        parts = rest
        single = purelib / f"{top}.pyi"
        if not rest and single.is_file() and not (purelib / top).is_dir():
            return single
    for root, suffix in roots:
        candidates = [root.joinpath(*parts, f"__init__{suffix}")]
        if parts:
            candidates.insert(0, root.joinpath(*parts).with_suffix(suffix))
        for candidate in candidates:
            if candidate.is_file():
                return candidate
    return None


def module_level(body: list, kind) -> list:
    """The module-level statements of `kind`, those under `if sys.version_info …` included, as
    `lotml bind` walks them; kept here so any revision of the binder is measured on one count."""
    found = []
    for node in body:
        if isinstance(node, kind):
            found.append(node)
        elif isinstance(node, ast.If):
            found.extend(module_level(node.body, kind) + module_level(node.orelse, kind))
    return found


def public_names(tree: ast.Module) -> set[str]:
    """The names a stub makes public: its `__all__`, or else what it defines or re-exports at
    module level, PEP 484's `import x as x` included, less the names starting with `_`."""
    listed: list[str] = []
    for node in module_level(tree.body, (ast.Assign, ast.AugAssign, ast.AnnAssign)):
        targets = node.targets if isinstance(node, ast.Assign) else [node.target]
        if any(isinstance(t, ast.Name) and t.id == "__all__" for t in targets) and isinstance(
            node.value, ast.List | ast.Tuple
        ):
            listed += [
                e.value
                for e in node.value.elts
                if isinstance(e, ast.Constant) and isinstance(e.value, str)
            ]
    if listed:
        return set(listed)
    names = set()
    declaring = (
        ast.FunctionDef,
        ast.AsyncFunctionDef,
        ast.ClassDef,
        ast.Assign,
        ast.AnnAssign,
        ast.Import,
        ast.ImportFrom,
    )
    for node in module_level(tree.body, declaring):
        if isinstance(node, ast.FunctionDef | ast.AsyncFunctionDef | ast.ClassDef):
            names.add(node.name)
        elif isinstance(node, ast.Assign):
            names.update(t.id for t in node.targets if isinstance(t, ast.Name))
        elif isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name):
            names.add(node.target.id)
        elif isinstance(node, ast.Import | ast.ImportFrom):
            names.update(
                a.asname for a in node.names if a.asname and a.asname == a.name.split(".")[-1]
            )
    return {n for n in names if not n.startswith("_")}


def bound_names(binder: ModuleType, name: str, stub: Path) -> set[str]:
    """The names `lotml bind` writes as functions of the interface of `name` from `stub`."""
    interface = binder.interface(name, stub)
    return {line[3 : line.index("(")] for line in interface.splitlines() if line.startswith("fn ")}


def measure(module: Module, binder: ModuleType, typeshed: Path | None, purelib: Path) -> Module:
    stub = stub_of(module, typeshed, purelib)
    if stub is None:
        return module
    root = typeshed if module.group == "stdlib" and typeshed is not None else purelib
    module.stub = stub.relative_to(root).as_posix()
    tree = ast.parse(stub.read_text(encoding="utf-8"), str(stub))
    public = public_names(tree)
    module.public = len(public)
    module.bound = sorted(public & bound_names(binder, module.name, stub))
    return module


def load_binder(path: Path) -> ModuleType:
    """The binder at `path`, loaded as a module of its own."""
    spec = importlib.util.spec_from_file_location("lotml_bind_measured", path)
    if spec is None or spec.loader is None:
        raise SystemExit(f"cannot load the binder {path}")
    binder = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(binder)
    return binder


def versions(modules: list[Module]) -> dict[str, str]:
    found = {}
    for distribution in sorted({m.distribution for m in modules}):
        try:
            found[distribution] = importlib.metadata.version(distribution)
        except importlib.metadata.PackageNotFoundError:
            found[distribution] = "not installed"
    return found


def dump(record: Record) -> str:
    data = {
        "label": record.label,
        "taken": record.taken,
        "corpus": record.corpus,
        "versions": record.versions,
        "modules": [
            {
                "name": m.name,
                "group": m.group,
                "distribution": m.distribution,
                "stub": m.stub,
                "public": m.public,
                "bound": m.bound,
            }
            for m in record.modules
        ],
    }
    return json.dumps(data, indent=2) + "\n"


def load(text: str) -> Record:
    data = json.loads(text)
    modules = [Module(**m) for m in data["modules"]]
    return Record(data["label"], modules, data["versions"], data["corpus"], data["taken"])


def markdown(records: list[Record]) -> str:
    records = sorted(records, key=lambda r: r.taken)
    latest = records[-1] if records else None
    lines = [
        "# Binding coverage: how much of Python `lotml bind` types",
        "",
        "Generated by `python -m lotml_harness.experiments.binding_coverage <label>`. For each",
        "module of `harness/binding-corpus.json`, the public names its stub declares and the share",
        "`lotml bind` binds typed, every stub read from a pinned distribution of the harness group",
        "`stubs` (specs/binding-coverage).",
        "",
        "| label | standard library | PyPI | all | measured on |",
        "| --- | ---: | ---: | ---: | --- |",
    ]
    for r in records:
        same = latest is not None and (r.corpus, r.versions) == (latest.corpus, latest.versions)
        on = "the latest corpus and versions" if same else "another corpus or other versions"
        shares = " | ".join(f"{r.share(group):.1%}" for group in [*GROUPS, None])
        lines.append(f"| {r.label} | {shares} | {on} |")
    lines.append("")
    for r in records:
        lines += [
            f"## {r.label}",
            "",
            "| module | from | public names | bound typed | share |",
            "| --- | --- | ---: | ---: | ---: |",
        ]
        for m in r.modules:
            source = m.distribution if m.stub else f"{m.distribution}, no stub found"
            lines.append(f"| {m.name} | {source} | {m.public} | {len(m.bound)} | {m.share:.1%} |")
        lines.append("")
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("label", help="what the measurement is kept under")
    parser.add_argument("--binder", type=Path, default=BINDER, help="the lotml_bind.py to measure")
    args = parser.parse_args(argv)
    if not LABEL.fullmatch(args.label):
        raise SystemExit("a label is letters, digits, `.`, `_` and `-`")
    text = CORPUS.read_text(encoding="utf-8")
    binder = load_binder(args.binder)
    typeshed = typeshed_root()
    purelib = Path(sysconfig.get_paths()["purelib"])
    modules = [measure(m, binder, typeshed, purelib) for m in corpus(text)]
    record = Record(
        args.label,
        modules,
        versions(modules),
        hashlib.sha256(text.encode("utf-8")).hexdigest()[:16],
        datetime.now(UTC).isoformat(timespec="seconds"),
    )
    RECORDS.mkdir(parents=True, exist_ok=True)
    (RECORDS / f"{record.label}.json").write_text(dump(record), encoding="utf-8", newline="\n")
    records = [load(p.read_text(encoding="utf-8")) for p in sorted(RECORDS.glob("*.json"))]
    REPORT.write_text(markdown(records), encoding="utf-8", newline="\n")
    print(markdown([record]).split("\n## ")[0])


if __name__ == "__main__":
    main()
