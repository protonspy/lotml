//! Generics, traits and `dyn` on the C target: a generic function or type is compiled once per
//! instantiation, a bounded type parameter calls the instance's own method, and a `dyn` value
//! calls through its type's table (R1.1, R1.2).

mod common;

use common::{parity, run_c_with};

const GENERICS: &str = include_str!("programs/generics.lotml");

#[test]
fn generics_traits_and_dyn_behave_as_in_python() {
    let run = parity("generics", GENERICS);
    assert!(run.stdout.starts_with("1 a 2.5\n"), "{}", run.stdout);
}

#[test]
fn generics_free_what_they_take() {
    let run = run_c_with("generics-free", GENERICS, true);
    assert!(run.stderr.contains("lotml: 0 cells live at exit"), "{}", run.stderr);
}

#[test]
fn recursion_that_grows_its_type_argument_is_refused_not_compiled_forever() {
    for (grown, what) in [("(x, x)", "a type this large"), ("[x]", "this many types")] {
        let source = format!(
            "fn nest[T](x: T, n: int) -> int:\n    if n == 0:\n        return 0\n    return 1 + nest({grown}, n - 1)\n\n\
             fn main():\n    print(nest(1, 3))\n"
        );
        let Err(errors) = lotml_c::compile(&source, std::path::Path::new("nest.lotml")) else {
            panic!("{grown}: an instance per depth, without end");
        };
        let messages: Vec<_> = errors.iter().map(|d| &d.message).collect();
        assert_eq!(errors.len(), 1, "{grown}: {messages:?}");
        assert_eq!(errors[0].code, "E0402", "{grown}: {messages:?}");
        assert!(errors[0].message.contains(what), "{grown}: {messages:?}");
    }
}

#[test]
fn a_type_whose_recursion_grows_its_type_arguments_is_refused() {
    for decl in [
        "type Nest[T] = Flat(T) | Deep(Nest[(T, T)])",
        "type L[T] = Nil | Cons(T, L[[T]])",
        "type A[T] = A0 | A1(B[[T]])\ntype B[T] = B0 | B1(A[T])",
    ] {
        let source = format!("{decl}\n\nfn main():\n    print(\"hi\")\n");
        let Err(errors) = lotml_c::compile(&source, std::path::Path::new("grow.lotml")) else {
            panic!("{decl}: a C type per depth, without end");
        };
        let messages: Vec<_> = errors.iter().map(|d| &d.message).collect();
        assert!(
            errors.iter().all(|d| d.code == "E0402" && d.message.contains("grows its type arguments")),
            "{decl}: {messages:?}"
        );
    }
    let regular = "type Tree[T] = Leaf(T) | Node([Tree[T]])\ntype Pair[T] = P(T, Pair[int]?)\n\n\
                   fn main():\n    t = Node([Leaf(1), Node([])])\n    p = P(\"a\", P(1, None))\n    print(t, p)\n";
    let run = parity("regular-recursive-types", regular);
    assert!(run.stdout.starts_with("Node("), "{}", run.stdout);
}
