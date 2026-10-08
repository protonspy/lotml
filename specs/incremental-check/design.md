# Incremental check — design

## One parse, then items

Serves R1.1, R1.2.

`parse(db, file)` stays one query over the whole text: its syntax errors are reported as today,
and its tree is the only tree. A new query `items(db, file)` turns each top-level item of that
tree into a salsa tracked struct:

- **identity**: the item's kind and name (a function's name, an `impl`'s target, a trait's name, a
  `test` block's title, an import's module). Two items with one kind and name are legal to parse
  and reported by the checker; salsa's creation-order disambiguator tells them apart.
- **tracked fields**: the item's syntax with every span moved to start at the item, the item's
  text from that start, and the item's start in the file. Moving an item changes only its start,
  which only the assembly reads.

The spans are moved by a walk in `lotml-syntax` that subtracts an offset from every span of an
item. Parsing each item's text on its own was the alternative, and it is rejected: recovery from
a syntax error depends on the text around it (an unclosed bracket ends at a dedent,
plans/frontend-robustness.md), so a second parse of an item could disagree with the first in
exactly the files R2.1 must report identically.

`Body` reads the source by span: literal text, and the start of a line where a fix inserts one.
An item carries its own text, which its relative spans index; a top-level item starts at the
start of a line, so the line arithmetic is unchanged.

## Signatures that compare equal across a body edit

Serves R1.3, R1.4, R1.5.

Two queries over the whole tree:

- `declarations(db, file)` runs `Program::collect` as `check_resolved_with` does today, with the
  file's interfaces, and keeps its diagnostics with spans in the file. It runs again on every edit.
- `signatures(db, file)` reads the `Program` from `declarations` and moves each signature's spans
  to start at the item that declares it. An edit inside a body leaves it equal, so salsa backdates
  it and no item check that read it runs again (R1.4); a changed declaration, import or interface
  changes it, and every item is checked again (R1.5).

`Program` and its parts compare by value for this, as `Interface` does since 2.2.

`check_item(db, item)` checks one item against `signatures(db, file)`, as the loop of
`check_resolved_with` checks it today, and returns its diagnostics, the type of each expression
and its local references, all relative to the item, with the count of type nodes it kept. A
body's diagnostics point only into its own item: a declaration elsewhere is named, not spanned.

## Assembly

Serves R2.1, R2.2, R2.3.

- `diagnostics(db, file)`: syntax errors, then the diagnostics of `declarations`, then each item's
  diagnostics in source order with their spans, labels and fixes moved to the item's start, sorted
  by start with the stable sort used today. The order they are produced in is the order
  `check_resolved_with` produces them in, so the sort ends in the same order (R2.1).
- `checked(db, file)`: the items' types and local references moved the same way, and the
  functions, types, methods, traits and imports of `declarations`. It is built only when a caller
  asks, as the editor does for hover; reporting diagnostics never builds it.
- The type-node limit: each item's check counts its nodes, and stops keeping types past the
  module's limit as `Checked::absorb` does today. The assembly adds the counts in source order;
  where the sum passes the limit, it reports `E0222` at that expression and types the rest as an
  error (R2.3). The types of one item are sorted by span, so moving them keeps the order the
  whole-file check counts them in.

A file's memos hold up to the limit for each item rather than for the file; the editor holds the
files it has open, as before. A file that reaches this ceiling is a note when it is built.

## Proving it

The whole-file check is the oracle. Over every corpus program
(`harness/results/corpus/corpus.jsonl`), the database's diagnostics and `Checked` equal the
whole-file check's, from an empty database and after an edit to each body. A salsa event log
counts the item checks an edit runs: one after an edit inside a body, all after one to a
signature. `check_speed` measures the result under a new label beside `before` and `after`.
