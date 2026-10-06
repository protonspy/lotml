//! The prelude and the methods of the built-in types (R35): every name the reference lists
//! types as it is used, and a call that misuses one is reported.

use lotml_check::check_source;

fn clean(source: &str) {
    let found = check_source(source);
    assert!(
        found.is_empty(),
        "expected no diagnostics in\n{source}\nfound {:#?}",
        found.iter().map(|d| (d.code, &d.message)).collect::<Vec<_>>()
    );
}

fn codes(source: &str) -> Vec<&'static str> {
    check_source(source).iter().map(|d| d.code).collect()
}

#[test]
fn every_prelude_function_types() {
    clean(concat!(
        "fn f(xs: [int], fs: [f64], s: str) -> int:\n",
        "    print(len(xs), len(s))\n",
        "    for i in range(3):\n        print(i)\n",
        "    for i in range(1, 9, 2):\n        print(i)\n",
        "    for i, x in enumerate(xs):\n        print(i + x)\n",
        "    pairs = list(zip(xs, fs))\n",
        "    seen = set(xs)\n",
        "    table = dict(pairs)\n",
        "    total = sum(xs) + abs(-1) + round(2.5) + ord(\"a\")\n",
        "    fraction = sum(fs) + abs(-1.5) + round(2.25, 1)\n",
        "    flag = bool(1) and any([True]) and all([True, False])\n",
        "    letter = chr(97)\n",
        "    q, r = divmod(7, 2)\n",
        "    big = pow(2, 10) + isqrt(17) + gcd(12, 18)\n",
        "    low = min(xs) + max(xs) + min(1, 2) + max(3, 4)\n",
        "    ordered = sorted(xs) + sorted(xs, key=lambda x: -x) + reversed(xs)\n",
        "    numbers = int(2.9) + int(float(3))\n",
        "    text = str(1) + str(2.0)\n",
        "    copies = list(xs)\n",
        "    doubled = list(map(lambda x: x * 2, xs))\n",
        "    kept = list(filter(lambda x: x > 1, xs))\n",
        "    wrapped = wrapping_add(1, 2) + wrapping_sub(1, 2) + wrapping_mul(2, 3)\n",
        "    var h = Heap(xs)\n",
        "    h.push(1)\n",
        "    smallest = h.pop_min() ?? 0\n",
        "    top = h.peek() ?? 0\n",
        "    return (total + big + low + numbers + q + r + len(ordered) + len(seen) + len(table)\n",
        "        + len(copies) + len(doubled) + len(kept) + wrapped + smallest + top + len(letter)\n",
        "        + len(text) + int(fraction) + len(pairs) + hash(s) + (1 if flag else 0))\n",
    ));
}

#[test]
fn every_string_method_types() {
    clean(concat!(
        "fn f(s: str) -> int:\n",
        "    parts = s.split(\",\") + s.split() + s.splitlines()\n",
        "    trimmed = s.strip() + s.lstrip() + s.rstrip(\"x\")\n",
        "    cased = s.lower() + s.upper() + s.title() + s.capitalize() + s.swapcase()\n",
        "    padded = s.zfill(5) + s.center(9) + s.ljust(4) + s.rjust(4, \"*\")\n",
        "    flags = s.startswith(\"a\") and s.endswith(\"b\") and s.isalpha() and s.isdigit()\n",
        "    more = s.isspace() or s.isupper() or s.islower() or s.isalnum() or s.isnumeric()\n",
        "    head, sep, tail = s.partition(\":\")\n",
        "    at = (s.find(\"x\") ?? -1) + (s.rfind(\"y\") ?? -1) + s.count(\"z\")\n",
        "    pair = s.split_once(\"=\") ?? (\"\", \"\")\n",
        "    n = (s.to_int() ?? 0) + int(s.to_float() ?? 0.0)\n",
        "    joined = \"-\".join(parts) + s.replace(\"a\", \"b\") + \"{}\".format(n)\n",
        "    return (len(trimmed + cased + padded + head + sep + tail + joined) + at + len(pair[0])\n",
        "        + (1 if flags or more else 0))\n",
    ));
}

#[test]
fn every_list_dict_and_set_method_types() {
    clean(concat!(
        "fn f(xs: [int], d: {str: int}, s: {int}) -> int:\n",
        "    var ys = list(xs)\n",
        "    ys.append(1)\n    ys.extend([2, 3])\n    ys.insert(0, 4)\n    ys.remove(4)\n",
        "    ys.sort()\n    ys.reverse()\n",
        "    last = ys.pop() ?? 0\n    first = ys.pop(0) ?? 0\n",
        "    where = (ys.index(2) ?? -1) + ys.count(1) + (ys.last() ?? 0) + (ys.find(lambda y: y > 1) ?? 0)\n",
        "    has = ys.contains(3)\n    other = ys.copy()\n    ys.clear()\n",
        "    var e = d.copy()\n",
        "    e.update({\"k\": 1})\n    e.setdefault(\"j\", 2)\n",
        "    got = (e.get(\"k\") ?? 0) + e.get(\"q\", 5) + (e.pop(\"k\") ?? 0) + e.pop(\"z\", 0)\n",
        "    size = len(e.keys()) + len(e.values()) + len(e.items())\n",
        "    found = e.contains(\"j\")\n    e.clear()\n",
        "    var t = s.copy()\n",
        "    t.add(1)\n    t.discard(2)\n    t.update({3})\n    t.remove(1)\n",
        "    picked = t.pop() ?? 0\n",
        "    mixed = t.union(s).intersection(s).difference({9})\n",
        "    sub = t.issubset(s) or t.issuperset(s)\n    t.clear()\n",
        "    return (last + first + where + len(other) + got + size + picked + len(mixed)\n",
        "        + (1 if has or found or sub else 0))\n",
    ));
}

#[test]
fn the_math_module_types_by_name_and_by_import() {
    clean(concat!(
        "import math\n",
        "from math import sqrt, floor, ceil, log, exp, sin, cos, pi, inf, isqrt, factorial\n\n",
        "fn f(x: f64, n: int) -> f64:\n",
        "    whole = floor(x) + ceil(x) + isqrt(n) + factorial(3) + math.gcd(4, 6) + math.comb(5, 2)\n",
        "    return (sqrt(x) + log(x) + exp(x) + sin(x) + cos(x) + pi + math.pow(x, 2.0)\n",
        "        + math.hypot(x, x) + math.atan2(x, x) + float(whole) + math.fabs(-x) + math.e\n",
        "        + (0.0 if x < inf else 1.0) + math.log2(x) + math.log10(x) + math.tan(x))\n",
    ));
}

#[test]
fn a_misused_prelude_function_is_reported() {
    assert_eq!(codes("fn f() -> int:\n    return len(1, 2)\n"), vec!["E0203"]);
    assert_eq!(codes("fn f() -> int:\n    return len(3)\n"), vec!["E0204"]);
    assert_eq!(codes("fn f() -> str:\n    return chr(\"a\")\n"), vec!["E0204"]);
    assert_eq!(codes("fn f(xs: [int]) -> int:\n    return sum([\"a\"])\n"), vec!["E0204"]);
    assert_eq!(codes("fn f() -> int:\n    return ord(1)\n"), vec!["E0204"]);
    assert_eq!(codes("fn f() -> int:\n    return math.floor(1.0)\n"), vec!["E0201"]);
    assert_eq!(codes("import math\n\nfn f() -> int:\n    return math.flor(1.0)\n"), vec!["E0205"]);
    assert_eq!(codes("from math import tau\n\nfn f() -> int:\n    return 1\n"), vec!["E0216"]);
}

#[test]
fn a_misused_method_is_reported() {
    assert_eq!(codes("fn f(s: str) -> str:\n    return s.split(1)[0]\n"), vec!["E0204"]);
    assert_eq!(codes("fn f(xs: [int]):\n    var ys = xs\n    ys.append(\"a\")\n"), vec!["E0204"]);
    assert_eq!(codes("fn f(xs: [int]):\n    var ys = xs\n    ys.insert(\"a\", 1)\n"), vec!["E0204"]);
    assert_eq!(codes("fn f(d: {str: int}) -> int:\n    return d.get(1) ?? 0\n"), vec!["E0204"]);
    assert_eq!(codes("fn f(s: {int}):\n    var t = s\n    t.add(\"a\")\n"), vec!["E0204"]);
    assert_eq!(codes("fn f(xs: [int]) -> int:\n    return xs.size()\n"), vec!["E0205"]);
}
