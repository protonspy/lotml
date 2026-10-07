//! Closures, methods, the `inout`, `sink` and `var` conventions, generics, traits and `dyn` on the
//! LLVM target: what the Python target prints, a callee changing its caller's value only through
//! `inout`, and every cell freed by the end (specs/llvm-parity R1.1, R1.2).

mod common;

use common::{frees_everything, parity};

const LAMBDAS: &str = include_str!("programs/lambdas.lotml");
const METHODS: &str = include_str!("programs/methods.lotml");
const GENERICS: &str = include_str!("programs/generics.lotml");

#[test]
fn lambdas_and_function_values_behave_as_in_python() {
    let Some([run, _]) = parity("lambdas", LAMBDAS) else { return };
    assert!(run.stdout.contains("\n10 2\n"), "a lambda captures a copy: {}", run.stdout);
    frees_everything("lambdas-free", LAMBDAS);
}

#[test]
fn methods_and_conventions_behave_as_in_python() {
    let Some([run, _]) = parity("methods", METHODS) else { return };
    assert!(run.stdout.starts_with("7 6 Counter(count=7, log=['+1', '+5', '+1'])"), "{}", run.stdout);
    frees_everything("methods-free", METHODS);
}

#[test]
fn a_value_passed_both_by_value_and_inout_keeps_its_own_count() {
    let source = "type Bag(items: [int])\n\nimpl Bag:\n    fn absorb(inout self, other: Bag):\n        \
                  self.items.extend(other.items)\n\n\
                  fn grow(a: [int], inout b: [int]):\n    b.append(len(a))\n\n\
                  fn ignore(a: [int], inout b: [int]):\n    b.append(0)\n\n\
                  fn main():\n    var xs = [1, 2, 3]\n    grow(xs, &xs)\n    print(xs)\n    \
                  var ys = [1, 2]\n    ignore(ys, &ys)\n    var b = Bag([1])\n    b.absorb(b)\n    print(b)\n    \
                  var c = Bag([2])\n    c.absorb(c)\n";
    let Some([run, _]) = parity("by-value-and-inout", source) else { return };
    assert_eq!(run.stdout, "[1, 2, 3, 3]\nBag(items=[1, 1])\n");
    frees_everything("by-value-and-inout-free", source);
}

#[test]
fn generics_traits_and_dyn_behave_as_in_python() {
    let Some([run, _]) = parity("generics", GENERICS) else { return };
    assert!(run.stdout.starts_with("1 a 2.5\n"), "{}", run.stdout);
    frees_everything("generics-free", GENERICS);
}

#[test]
fn recursion_that_grows_its_type_argument_is_refused_not_compiled_forever() {
    for (grown, what) in [("(x, x)", "a type this large"), ("[x]", "this many types")] {
        let source = format!(
            "fn nest[T](x: T, n: int) -> int:\n    if n == 0:\n        return 0\n    return 1 + nest({grown}, n - 1)\n\n\
             fn main():\n    print(nest(1, 3))\n"
        );
        let Err(errors) = lotml_llvm::compile(&source, std::path::Path::new("nest.lot")) else {
            panic!("{grown}: an instance per depth, without end");
        };
        let messages: Vec<_> = errors.iter().map(|d| &d.message).collect();
        assert_eq!(errors.len(), 1, "{grown}: {messages:?}");
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
        let Err(errors) = lotml_llvm::compile(&source, std::path::Path::new("grow.lot")) else {
            panic!("{decl}: a type per depth, without end");
        };
        let messages: Vec<_> = errors.iter().map(|d| &d.message).collect();
        assert!(errors.iter().all(|d| d.message.contains("grows its type arguments")), "{decl}: {messages:?}");
    }
    let regular = "type Tree[T] = Leaf(T) | Node([Tree[T]])\ntype Pair[T] = P(T, Pair[int]?)\n\n\
                   fn main():\n    t = Node([Leaf(1), Node([])])\n    p = P(\"a\", P(1, None))\n    print(t, p)\n";
    if let Some([run, _]) = parity("regular-recursive-types", regular) {
        assert!(run.stdout.starts_with("Node("), "{}", run.stdout);
    }
    frees_everything("regular-recursive-types-free", regular);
}
