//! Lists, tuples, ranges, comprehensions and the prelude over them on the C target (R1.2, R1.3).

mod common;

use common::{parity, run_c_with};

#[test]
fn lists_index_slice_sort_and_print_as_python() {
    let run = parity(
        "lists",
        "fn squares(n: int) -> [int]:\n    return [i * i for i in range(n) if i % 2 == 1]\n\n\
         fn total(xs: [f64]) -> f64:\n    return sum(xs)\n\n\
         fn main():\n    xs = [3, 1, 2]\n    \
         print(xs, len(xs), xs[0], xs[-1], xs[1:], xs[::-1], sorted(xs), sorted(xs, reverse=True), 2 in xs, 5 not in xs)\n    \
         print(squares(10), sum(squares(10)), min(xs), max(xs), sum([0.1, 0.2, 0.3]), total([1e100, 1.0, -1e100]))\n    \
         print([1, 2] + [3], [0] * 3, [\"s\"] * 2, str([1.5, 2.0]), f\"{xs}\", [[]], sorted([\"b\", \"a\", \"C\"]), sorted([(2, \"a\"), (1, \"b\"), (1, \"a\")]))\n",
    );
    assert!(
        run.stdout.starts_with("[3, 1, 2] 3 3 2 [1, 2] [2, 1, 3] [1, 2, 3] [3, 2, 1] True True\n"),
        "{}",
        run.stdout
    );
}

#[test]
fn list_methods_change_a_var_in_place() {
    parity(
        "list-methods",
        "fn main():\n    xs = [3, 1, 2]\n    var ys: [int] = []\n    for x in xs:\n        ys.append(x * 10)\n    \
         ys.insert(0, 7)\n    ys.extend([8, 9])\n    ys.sort()\n    ys.reverse()\n    ys.remove(8)\n    \
         print(ys, ys.count(7), ys.contains(30), list(range(3)), list(reversed([1, 2, 3])), ys == [30, 20, 10, 9, 7], [1, 2] < [1, 3])\n    \
         var grid = [[0, 1], [2, 3]]\n    grid[1][0] = 5\n    grid[0].append(9)\n    \
         print(grid, [[c * 2 for c in row] for row in grid], [r[0] for r in grid])\n    \
         ys.clear()\n    print(ys, len(ys))\n",
    );
}

#[test]
fn tuples_enumerate_and_zip_unpack_as_python() {
    parity(
        "tuples",
        "fn main():\n    pairs = [(1, \"a\"), (2, \"b\")]\n    for i, (n, s) in enumerate(pairs):\n        print(i, n, s)\n    \
         for a, b in zip([1, 2, 3], [\"x\", \"y\"]):\n        print(a, b)\n    \
         print(pairs, (1,), (1, 2.5, \"q\", True, None), list(enumerate(\"ab\")), list(zip([1], [2])))\n    \
         t = (1, \"x\")\n    a, b = t\n    print(a, b, t[0], t[1], len(t))\n    \
         for k in reversed(range(3)):\n        print(k)\n",
    );
}

#[test]
fn strings_split_and_join_through_lists() {
    parity(
        "split-join",
        "fn main():\n    xs = [3, 1, 2]\n    words = \"the quick  brown fox\".split()\n    \
         print(words, \"a,b,,c\".split(\",\"), \"-\".join(words), \", \".join([\"x\", \"y\"]), \"x y\".split(\" \", 1), \"l1\\nl2\\n\".splitlines(), \"k=v\".partition(\"=\"))\n    \
         print(any([False, True]), all([True, False]), any(x > 2 for x in xs), all(x > 0 for x in xs), [c for c in \"héllo\" if c != \"l\"])\n",
    );
}

#[test]
fn list_mistakes_stop_the_program() {
    for (name, body) in [
        ("index", "print(xs[3])"),
        ("empty-min", "print(min([0][1:]))"),
        ("remove", "var ys = xs\n    ys.remove(9)"),
        ("step", "print(xs[::0])"),
    ] {
        let source = format!("fn main():\n    xs = [1, 2, 3]\n    print(\"before\")\n    {body}\n");
        let run = parity(&format!("list-panic-{name}"), &source);
        assert_eq!(run.code, Some(101), "{name}");
    }
}

#[test]
fn a_loop_body_that_changes_its_list_cannot_free_what_the_loop_reads() {
    let source = "fn main():\n    var xs = [\"a\" + str(1), \"a\" + str(2), \"a\" + str(3)]\n    \
                  for x in reversed(xs):\n        xs.clear()\n        print(x)\n    \
                  var zs = [1, 2, 3]\n    for z in reversed(zs):\n        zs = [9]\n        print(z)\n    \
                  var ws = [\"w\" + str(1), \"w\" + str(2)]\n    for i, w in enumerate(reversed(ws)):\n        \
                  ws.clear()\n        print(i, w)\n    \
                  var ps = [1, 2, 3]\n    var qs = [\"q\" + str(1), \"q\" + str(2), \"q\" + str(3)]\n    \
                  for p, q in zip(ps, qs):\n        qs.clear()\n        ps = []\n        print(p, q)\n    \
                  print(xs, zs, ws, ps, qs)\n";
    let run = parity("loop-changes-its-list", source);
    assert!(run.stdout.starts_with("a3\na2\na1\n3\n2\n1\n0 w2\n1 w1\n1 q1\n"), "{}", run.stdout);
    let counted = run_c_with("loop-changes-its-list-free", source, true);
    assert!(counted.stderr.contains("lotml: 0 cells live at exit"), "{}", counted.stderr);
}
