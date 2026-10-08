//! Strings on the LLVM target: what the Python target prints, CPython's `repr` and format
//! mini-language included (specs/llvm-parity R1.2, R1.3).

mod common;

use common::{frees_everything, parity};

#[test]
fn indexing_slicing_and_case_follow_python() {
    let Some([run, _]) = parity(
        "str-basics",
        "fn shout(s: str) -> str:\n    return s.upper() + \"!\"\n\n\
         fn main():\n    s = \"Hello, World\"\n    \
         print(s, len(s), s[0], s[-1], s[1:4], s[::-1], s[::2], s[7:], s[:-7], s[100:], s[-100:3])\n    \
         print(s.lower(), s.upper(), shout(\"héllo ß\"), s.swapcase(), \"hello world\".title(), \"hELLO\".capitalize())\n    \
         print(len(\"héllo\"), \"héllo\"[1], \"héllo\"[1:3], \"naïve café\"[::-1])\n",
    ) else {
        return;
    };
    assert!(run.stdout.starts_with("Hello, World 12 H d ell dlroW ,olleH Hlo ol World Hello  Hel\n"), "{}", run.stdout);
}

#[test]
fn methods_and_operators_follow_python() {
    parity(
        "str-methods",
        "fn main():\n    s = \"Hello, World\"\n    \
         print(\"  pad  \".strip() + \"|\", \"xxhixx\".strip(\"x\"), \"  a \".lstrip() + \"|\", \" a  \".rstrip() + \"|\")\n    \
         print(s.startswith(\"Hell\"), s.endswith(\"d\"), s.replace(\"l\", \"L\"), s.replace(\"l\", \"\", 1), s.count(\"l\"), \"aaa\".count(\"aa\"))\n    \
         print(\"abc\".isalpha(), \"ab1\".isalpha(), \"123\".isdigit(), \"\".isdigit(), \" \\t\\n\".isspace(), \"ABC\".isupper(), \"abc1\".islower())\n    \
         print(\"ab\" + \"cd\", \"ab\" * 3, 3 * \"x\", \"-\" * 0, \"a\" < \"b\", \"abc\" < \"abd\", \"b\" > \"abc\", \"x\" == \"x\", \"x\" != \"y\", \"ell\" in s, \"z\" not in s)\n    \
         print(ord(\"A\"), ord(\"é\"), chr(65), chr(233), str(42), str(-1.5), str(True), str(None), str(\"s\"))\n    \
         print(\"42\".zfill(5), \"-42\".zfill(5), \"ab\".center(6, \"*\"), \"ab\".ljust(4) + \"|\", \"ab\".rjust(4), \"é\".center(4, \"-\"))\n    \
         var built = \"\"\n    for i in range(3):\n        built = built + str(i)\n    print(built, built.replace(\"\", \"-\"))\n",
    );
}

#[test]
fn f_strings_use_the_format_mini_language() {
    parity(
        "f-strings",
        "fn main():\n    name = \"ana\"\n    age = 30\n    ratio = 2.0 / 3.0\n    \
         print(f\"{name} is {age} years old, {ratio:.2f}, {ratio:.3}, {age:>5}|{age:<5}|{age:^5}|{age:05}|{age:+d}\")\n    \
         print(f\"{1234567:,} {1234567.891:,.2f} {255:x} {255:#X} {255:b} {255:o} {0.5:%} {0.25:.1%} {1e-7:g} {123456789.0:g} {1.5:e} {name!r} {name:>6} {name:*^7}\")\n    \
         print(f\"{3.0} {1e20} {-0.0:+} {7:.2f} {True} {True:>5} {None} {'q'}{{literal}}\", f\"{age * 2 + 1}\")\n    \
         print(f\"{-7:=6} {-7:06} {1234.5:.3} {0.0001:.2} {1e16:.17g} {12:_b} {-0.0:z.1f} {1e100:.2e} {2.5:.0f} {3.5:.0f}\")\n",
    );
}

#[test]
fn a_string_s_repr_is_python_s() {
    parity(
        "str-repr",
        "fn main():\n    a = \"it's\"\n    b = 'say \"hi\"'\n    c = \"both ' and \\\"\"\n    d = \"tab\\there\\nnew\\\\\"\n    \
         e = \"\\x00\\x7f é\"\n    print(f\"{a!r} {b!r} {c!r} {d!r} {e!r}\")\n",
    );
}

#[test]
fn a_string_is_walked_a_character_at_a_time() {
    let Some([run, _]) = parity(
        "str-walk",
        "fn main():\n    var n = 0\n    for c in \"héllo, wörld\":\n        if c == \",\":\n            continue\n        \
         if c == \"r\":\n            break\n        n += 1\n        print(c, end=\"\")\n    print(\"\", n)\n    \
         for c in \"\":\n        print(\"never\", c)\n",
    ) else {
        return;
    };
    assert_eq!(run.stdout, "héllo wö 8\n");
}

#[test]
fn string_mistakes_stop_the_program() {
    for (name, expr) in [("index", "s[10]"), ("chr", "chr(-1)"), ("ord", "ord(s)")] {
        let source = format!("fn main():\n    s = \"abc\"\n    print(\"before\")\n    print({expr})\n");
        if let Some(runs) = parity(&format!("str-panic-{name}"), &source) {
            assert!(runs.iter().all(|r| r.code == Some(101)), "{name}");
        }
    }
}

#[test]
fn strings_are_freed() {
    frees_everything(
        "free-strings",
        "fn shout(s: str) -> str:\n    return s.upper() + \"!\"\n\n\
         fn main():\n    var acc = \"\"\n    for w in \"a bb ccc\".split():\n        acc = acc + shout(w) + \",\"\n        \
         if len(acc) > 100:\n            continue\n    name = \"ana\"\n    print(f\"{name!r:>8}|{acc}|{len(acc)}\")\n    \
         parts = [p.strip() for p in \" x , y ,z\".split(\",\") if p != \"\"]\n    print(\"-\".join(parts), parts[::-1], sorted(parts))\n    \
         for c in \"héllo\":\n        if c == \"l\":\n            break\n        print(c)\n",
    );
}
