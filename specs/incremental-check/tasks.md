# Incremental check — tasks

## 1 · Items, signatures and one check per item

- [x] 1.1 (Unit) Move every span of an item by an offset in `lotml-syntax`, so that an item parsed at one position and moved equals the same item parsed at another — R1.2
- [x] 1.2 (Unit) Split a parsed file into items in `lotml-db`: tracked structs identified by kind and name, holding the moved syntax, the item's text and its start — R1.1, R1.2
  _Depends 1.1_
- [ ] 1.3 (Unit) Keep the file's declarations with their spans in the file, and its signatures with spans relative to the declaring item, equal across an edit inside a body — R1.3, R1.5
  _Depends 1.2_
- [ ] 1.4 (TDD) Check each item in a query of its own against the signatures, shown first by a failing test that an edit inside one body runs one item check and an edit to a signature runs all — R1.1, R1.4, R1.5
  _Depends 1.3_

## 2 · The file's report, and what it costs

- [ ] 2.1 (TDD) Assemble a file's diagnostics and `Checked` from its items moved to their positions, with `E0222` over the count in source order, shown equal to the whole-file check over every corpus program, from an empty database and after an edit to each body — R2.1, R2.2, R2.3
  _Depends 1.4_
- [ ] 2.2 (Unit) Measure with `check_speed` under a new label, and record in `docs/wiki/pages/compiler-performance.md` whether the edit and the empty database meet their bounds — R3.1, R3.2
  _Depends 2.1_
