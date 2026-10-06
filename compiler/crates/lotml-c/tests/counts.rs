//! Counting on the C target: values copied only when changed while shared, and every cell freed
//! by the time the program ends (R3.1, R3.5, R3.6).

mod common;

use common::{parity, run_c_with, run_python};

const VALUES: &str = "fn grow(var xs: [int]) -> [int]:\n    xs.append(99)\n    return xs\n\n\
fn first_long(words: [str]) -> str:\n    for w in words:\n        if len(w) > 3:\n            return w\n    return \"none\"\n\n\
fn main():\n    xs = [1, 2, 3]\n    var ys = xs\n    ys.append(4)\n    print(xs, ys)\n    var g = [[1], [2]]\n    var h = g\n    \
h[0].append(5)\n    print(g, h)\n    zs = grow(xs)\n    print(xs, zs)\n    var keep = [\"abc\" * 2, \"d\"]\n    s = keep[0]\n    \
keep.clear()\n    print(s, keep)\n    var total = \"\"\n    for i in range(5):\n        total = total + str(i)\n        if i == 3:\n            \
break\n    print(total, first_long([\"a\", \"bb\", \"cccc\", \"dd\"]), first_long([]))\n    var big: [int] = []\n    for i in range(200000):\n        \
big.append(i)\n    print(len(big), big[-1])\n    pairs = [(str(i), [i]) for i in range(3)]\n    for name, nums in pairs:\n        \
print(name, nums)\n    t = pairs[1]\n    print(t, t[0] + \"!\")\n";

#[test]
fn a_value_changed_while_shared_is_copied_first() {
    let run = parity("values", VALUES);
    assert_eq!(
        run.stdout,
        "[1, 2, 3] [1, 2, 3, 4]\n[[1], [2]] [[1, 5], [2]]\n[1, 2, 3] [1, 2, 3, 99]\nabcabc []\n0123 cccc none\n\
         200000 199999\n0 [0]\n1 [1]\n2 [2]\n('1', [1]) 1!\n"
    );
}

/// Every cell the program allocated is freed by its end: what a counting build reports.
fn frees_everything(name: &str, source: &str) {
    let python = run_python(name, source);
    let c = run_c_with(name, source, true);
    assert_eq!(c.stdout, python.stdout, "{name}: {}", c.stderr);
    assert!(c.stderr.contains("lotml: 0 cells live at exit"), "{name}: {}\n--- the C ---\n{}", c.stderr, c.c);
}

#[test]
fn every_cell_is_freed_by_the_end() {
    frees_everything("free-values", VALUES);
    frees_everything(
        "free-strings",
        "fn shout(s: str) -> str:\n    return s.upper() + \"!\"\n\n\
         fn main():\n    var acc = \"\"\n    for w in \"a bb ccc\".split():\n        acc = acc + shout(w) + \",\"\n        \
         if len(acc) > 100:\n            continue\n    name = \"ana\"\n    print(f\"{name!r:>8}|{acc}|{len(acc)}\")\n    \
         parts = [p.strip() for p in \" x , y ,z\".split(\",\") if p != \"\"]\n    print(\"-\".join(parts), parts[::-1], sorted(parts))\n    \
         for c in \"héllo\":\n        if c == \"l\":\n            break\n        print(c)\n",
    );
    frees_everything(
        "free-nested",
        "fn build(n: int) -> [[str]]:\n    var rows: [[str]] = []\n    for i in range(n):\n        \
         rows.append([str(j) for j in range(i)])\n    return rows\n\n\
         fn main():\n    var grid = build(4)\n    grid[2][1] = \"x\"\n    grid[3].append(\"y\")\n    \
         copy = grid\n    grid[0].append(\"z\")\n    print(grid, copy)\n    \
         for i, row in enumerate(grid):\n        for j, cell in enumerate(row):\n            if cell == \"x\":\n                print(i, j)\n    \
         pairs = list(zip([\"a\", \"b\"], [[1], [2, 3]]))\n    print(pairs, pairs[1][1], max([\"b\", \"a\"]), min(grid))\n    \
         var n = 0\n    while True:\n        n += 1\n        if n > 3:\n            break\n        grid.reverse()\n    print(grid[0])\n",
    );
}
