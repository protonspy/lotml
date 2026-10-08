//! A Python stub read as a LotML interface (specs/rust-binder R1, adr:0012): its module-level
//! functions, and the names typeshed writes as methods of a module-level instance, `randint =
//! _inst.randint`, bound from those methods. A function whose types LotML cannot express is listed
//! in a comment with the reason, never bound half-way; a type no LotML type describes is `PyObject`
//! (adr:0031).

/// Why a stub was not bound.
#[derive(Debug, PartialEq, Eq)]
pub struct Refused(pub String);

/// The interface of `module` from the text of its stub, the first line naming `said`, the source.
pub fn interface(_module: &str, _stub: &str, _said: &str) -> Result<String, Refused> {
    Err(Refused("not bound".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bound(stub: &str) -> String {
        interface("m", stub, "m.pyi, the stub given").unwrap_or_else(|Refused(why)| panic!("{why}"))
    }

    fn functions(stub: &str) -> Vec<String> {
        bound(stub).lines().filter(|l| l.starts_with("fn ")).map(str::to_string).collect()
    }

    fn one(stub: &str) -> String {
        let mut found = functions(stub);
        assert_eq!(found.len(), 1, "{found:?}");
        found.remove(0)
    }

    #[test]
    fn the_interface_says_where_it_came_from_and_how_to_read_it() {
        let text = bound("def f(x: int) -> int: ...\n");
        assert_eq!(
            text,
            "# The Python module `m`, bound by `lotml bind` from m.pyi, the stub given.\n\
             # Do not edit: run `lotml bind` again.\n\
             # Every function returns `T ! PyError`: a stub does not say what a call raises.\n\
             # A parameter written `= todo()` is optional: Python supplies its default.\n\
             # A `PyObject` is a value no LotML type describes: convert it with `value()`.\n\
             fn f(x: int) -> int ! PyError\n"
        );
    }

    #[test]
    fn simple_types_map_to_lotml_s() {
        assert_eq!(
            one("def f(a: int, b: float, c: str, d: bool, e: bytes) -> None: ...\n"),
            "fn f(a: int, b: f64, c: str, d: bool, e: bytes) -> None ! PyError"
        );
    }

    #[test]
    fn an_optional_is_marked_once_and_any_other_union_is_a_pyobject() {
        assert_eq!(
            one("def f(a: int | None, b: Optional[str]) -> int | None: ...\n"),
            "fn f(a: int?, b: str?) -> int? ! PyError"
        );
        assert_eq!(one("def f(a: int | str) -> None: ...\n"), "fn f(a: PyObject) -> None ! PyError");
        assert_eq!(one("def f(a: int | str | None) -> None: ...\n"), "fn f(a: PyObject?) -> None ! PyError");
        assert_eq!(one("def f(a: Optional[int | None]) -> None: ...\n"), "fn f(a: int?) -> None ! PyError");
    }

    #[test]
    fn a_parameter_takes_an_abstract_collection_and_a_result_is_the_concrete_one() {
        assert_eq!(
            one(
                "def f(a: Sequence[int], b: Mapping[str, float], c: AbstractSet[str], d: Iterable[str]) -> list[int]: ...\n"
            ),
            "fn f(a: [int], b: {str: f64}, c: {str}, d: [str]) -> [int] ! PyError"
        );
        assert_eq!(one("def f() -> Sequence[int]: ...\n"), "fn f() -> PyObject ! PyError");
        assert_eq!(
            one("def f(a: dict[str, int], b: set[int], c: frozenset[str], d: List[str]) -> Dict[str, int]: ...\n"),
            "fn f(a: {str: int}, b: {int}, c: {str}, d: [str]) -> {str: int} ! PyError"
        );
    }

    #[test]
    fn a_tuple_is_fixed_and_one_of_any_length_is_a_pyobject() {
        assert_eq!(one("def f(a: tuple[int, str]) -> Tuple[int]: ...\n"), "fn f(a: (int, str)) -> (int) ! PyError");
        assert_eq!(one("def f(a: tuple[int, ...]) -> None: ...\n"), "fn f(a: PyObject) -> None ! PyError");
    }

    #[test]
    fn what_a_parameter_accepts_is_what_lotml_passes_and_a_result_of_it_is_a_pyobject() {
        assert_eq!(
            one("def f(a: SupportsIndex, b: SupportsFloat, c: StrPath, d: ReadableBuffer) -> SupportsInt: ...\n"),
            "fn f(a: int, b: f64, c: str, d: bytes) -> PyObject ! PyError"
        );
    }

    #[test]
    fn a_name_lotml_has_no_type_for_is_a_pyobject_and_so_is_an_annotation_left_out() {
        assert_eq!(
            one("def f(a: Callable[[int], int], b, c: 'int') -> Any: ...\n"),
            "fn f(a: PyObject, b: PyObject, c: PyObject) -> PyObject ! PyError"
        );
        assert_eq!(one("def f(a: typing.Optional[builtins.int]): ...\n"), "fn f(a: int?) -> PyObject ! PyError");
    }

    #[test]
    fn defaults_are_written_as_python_writes_them_or_left_to_python() {
        assert_eq!(
            one("def f(a: int = 1, b: float = 1.5, c: str = 'x', d: bool = True, e: int | None = None) -> None: ...\n"),
            "fn f(a: int = 1, b: f64 = 1.5, c: str = \"x\", d: bool = True, e: int? = None) -> None ! PyError"
        );
        assert_eq!(
            one("def f(a: int = -1, b: bytes = b'x', c: int = ..., d: int = SOME, e: complex = 1j) -> None: ...\n"),
            "fn f(a: int = todo(), b: bytes = todo(), c: int = todo(), d: int = todo(), e: PyObject = todo()) -> None ! PyError"
        );
        assert_eq!(
            one("def f(a: int = 0x10, b: int = 1_000) -> None: ...\n"),
            "fn f(a: int = 16, b: int = 1000) -> None ! PyError"
        );
    }

    #[test]
    fn a_float_default_is_python_s_repr() {
        let written = |literal: &str| {
            let line = one(&format!("def f(a: float = {literal}) -> None: ...\n"));
            line["fn f(a: f64 = ".len()..line.len() - ") -> None ! PyError".len()].to_string()
        };
        for (literal, repr) in [
            ("1.0", "1.0"),
            ("100.0", "100.0"),
            ("0.5", "0.5"),
            ("0.0001", "0.0001"),
            ("0.00001", "1e-05"),
            ("1e16", "1e+16"),
            ("1e15", "1000000000000000.0"),
            ("123456789012345678.0", "1.2345678901234568e+17"),
            ("1.5e-7", "1.5e-07"),
            ("1e300", "1e+300"),
            ("0.0", "0.0"),
            ("1e999", "inf"),
        ] {
            assert_eq!(written(literal), repr, "{literal}");
        }
    }

    #[test]
    fn a_string_default_escapes_what_would_end_it_or_its_line() {
        assert_eq!(
            one("def f(a: str = 'say \"hi\"\\\\\\n') -> None: ...\n"),
            "fn f(a: str = \"say \\\"hi\\\"\\\\\\n\") -> None ! PyError"
        );
        assert_eq!(
            one("def f(a: str = '\\t\\r\\x07\\u2028\\x85') -> None: ...\n"),
            "fn f(a: str = \"\\x09\\x0d\\x07\\u2028\\x85\") -> None ! PyError"
        );
    }

    #[test]
    fn star_parameters_are_left_to_python_and_the_named_ones_bound() {
        assert_eq!(
            one("def f(a: int, /, b: str, *args: int, c: bool = False, **kw: int) -> None: ...\n"),
            "fn f(a: int, b: str, c: bool = False) -> None ! PyError"
        );
    }

    #[test]
    fn version_blocks_are_walked_and_the_first_definition_wins() {
        let stub = "import sys\nif sys.version_info >= (3, 12):\n    def f(a: int) -> int: ...\nelif sys.version_info >= (3, 10):\n    def f(a: str) -> str: ...\n    def g() -> None: ...\nelse:\n    def h() -> None: ...\n";
        assert_eq!(
            functions(stub),
            ["fn f(a: int) -> int ! PyError", "fn g() -> None ! PyError", "fn h() -> None ! PyError"]
        );
    }

    #[test]
    fn private_names_classes_and_nested_functions_are_not_bound() {
        let stub = "def _p() -> None: ...\nclass C:\n    def m(self) -> None: ...\ndef f() -> None:\n    def inner() -> None: ...\n";
        assert_eq!(functions(stub), ["fn f() -> None ! PyError"]);
    }

    #[test]
    fn an_overload_and_a_coroutine_are_listed_with_the_reason() {
        let stub = "@overload\ndef p(x: int) -> int: ...\n@typing.overload\ndef p(x: str) -> str: ...\nasync def c() -> None: ...\ndef f() -> None: ...\n";
        let text = bound(stub);
        assert!(
            text.ends_with(
                "fn f() -> None ! PyError\n\n# Not bound:\n#   p: it is overloaded\n#   c: it is a coroutine\n"
            ),
            "{text}"
        );
    }

    #[test]
    fn a_name_written_as_an_instance_s_method_is_bound_from_the_method() {
        let stub = "class Random:\n    def randint(self, a: int, b: int) -> int: ...\n    @staticmethod\n    def make(seed: int) -> float: ...\n    @classmethod\n    def named(cls, name: str) -> str: ...\n    def __init__(self) -> None: ...\n_inst: Random\nrandint = _inst.randint\nmake = _inst.make\nnamed = _inst.named\ngone = _inst.missing\n";
        let text = bound(stub);
        let found: Vec<&str> = text.lines().filter(|l| l.starts_with("fn ")).collect();
        assert_eq!(
            found,
            [
                "fn randint(a: int, b: int) -> int ! PyError",
                "fn make(seed: int) -> f64 ! PyError",
                "fn named(name: str) -> str ! PyError"
            ]
        );
        assert!(text.contains("#   gone: `Random` holds no `missing` in this stub\n"), "{text}");
    }

    #[test]
    fn a_method_of_an_instance_a_version_block_declares_counts_the_first_written() {
        let stub = "import sys\nclass R:\n    if sys.version_info >= (3, 12):\n        def r(self, a: int) -> int: ...\n    else:\n        def r(self, a: str) -> str: ...\n_i: R\nr = _i.r\n";
        assert_eq!(functions(stub), ["fn r(a: int) -> int ! PyError"]);
    }

    #[test]
    fn a_name_bound_once_is_not_bound_again() {
        assert_eq!(
            functions("def f(a: int) -> int: ...\ndef f(a: str) -> str: ...\n"),
            ["fn f(a: int) -> int ! PyError"]
        );
    }

    #[test]
    fn the_source_named_can_never_add_a_line() {
        let text = interface("m", "def f() -> None: ...\n", "x\nfn evil() -> int ! PyError\r\u{85}y").unwrap();
        assert_eq!(
            text.lines().next(),
            Some("# The Python module `m`, bound by `lotml bind` from x?fn evil() -> int ! PyError??y.")
        );
        assert_eq!(text.lines().filter(|l| l.starts_with("fn ")).count(), 1);
    }

    #[test]
    fn a_stub_that_does_not_parse_is_refused_saying_where() {
        let Err(Refused(why)) = interface("m", "def f(:\n", "m.pyi") else { panic!("not Python") };
        assert!(why.contains("line 1"), "{why}");
    }
}
