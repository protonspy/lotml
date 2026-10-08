---
autonomy: auto
ci: wait
---

# Incremental check — requirements

## Purpose

An edit to one body costs what the body costs to check, not what the file costs. `lotml-db`
checks a whole file in one query whose results carry offsets from the start of the file, so a
one-line edit checks every function again, and a function whose text did not change still gets
new results because its position moved (n-0096). A file's interfaces are already read once
(plans/build-and-check-speed.md 2.2); in a generated file of 2254 lines the check after a body
edit still takes 82% of a check from an empty database (`harness/results/check-speed.md`).
Signatures are fully annotated, so a body can be checked against the file's signatures without
any other body (docs/wiki/pages/compiler-performance.md).

## Requirements

### 1 · One check per item

- **R1.1** The database shall check each item of a file (a function, each method of an `impl` or a trait, a record's field defaults, a `test` block) in a query of its own that reads the item's syntax and the file's signatures and no other item's body.
- **R1.2** The database shall keep each item's syntax and the results of each item's check with every span relative to the start of the item, so that an item moved without a change to its text has the same syntax and the same results.
- **R1.3** The database shall keep the file's signatures (its types, functions, methods, traits and the functions it imports) with every span relative to the start of the item that declares it.
- **R1.4** When an edit changes the text inside one item's body and nothing the file declares or imports, the database shall run that item's check again and reuse the result of every other item's check.
- **R1.5** When an edit changes what the file declares or imports, or the file's interfaces change, the database shall check every item again against the new signatures.

### 2 · The file's report, assembled from its items

- **R2.1** The database shall report for a file the diagnostics, in the order, that `lotml_check::check_source_with` reports for the same text and interfaces, each item's diagnostics moved to the item's position in the file.
- **R2.2** The database shall give for a file the `Checked` that `lotml_check::check_resolved_with` gives for the same text and interfaces: the type of every expression, what each local's name refers to, and the functions, types, methods, traits and imported functions it declares.
- **R2.3** If the type nodes of a file's expressions, counted item by item in source order, pass the module's limit, then the database shall report `E0222` once, at the expression where the count passes the limit, and type every expression past it as an error, as the whole-file check does.

### 3 · What it costs

- **R3.1** When one line of one body of the file that `check_time` generates is edited, the database shall report the file's diagnostics in no more than half the time a check from an empty database takes, as `check_speed` measures both.
- **R3.2** When a file is checked from an empty database, the database shall take no more than 25% longer than the whole-file check took on the same machine, as `check_speed` records it.

## Out of scope

- Parsing a file a piece at a time: the whole file is parsed on every edit, 2.0 ms for 2254 lines.
- Checking an item again only when a signature it uses changed: any change to the file's
  signatures checks every item again (R1.5).
- The backends: `lotml build` and `lotml run` keep calling the whole-file check, which stays the
  oracle R2.1 and R2.2 are tested against.
