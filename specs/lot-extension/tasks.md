# Lot extension — tasks

## 1 · The compiler

- [x] 1.1 (Unit) Add `files::is_source`, true for `.lot` and `.lotml`, and use it in the directory walk and in the language server's reload of a changed file — R1.1
- [x] 1.2 (Unit) Refuse in `files::expand` two found files that differ only in `.lot` against `.lotml`, naming both, and give the MCP server and the language server `files::sources`, the same search without that check — R1.2
  _Depends 1.1_
- [x] 1.3 (Unit) Name `.lot` and `.lotml` in the CLI's help, the MCP tools' descriptions, the messages and the language reference, call the compiler LotML in its `about`, and append `.partial` to the whole file name in the MCP server's atomic write — R1.3, R1.4, R3.1
  _Depends 1.1_
- [x] 1.4 (Unit) Keep a `.lot` file's frames in the Python runtime's tracebacks, and list `lot` in the tree-sitter grammar's file types — R1.1

## 2 · What is written

- [x] 2.1 (Unit) Write the guide as `lotml.guide.lot` in `lotml init`, replacing a `lotml.guide.lotml` and reporting the rename, and name that file and both extensions in the `AGENTS.md` block — R2.1, R2.2
  _Depends 1.1_
- [x] 2.2 (Unit) Take `.lot` beside `.lotml` in the harness's safe layer and in the run's workspace snapshots — R2.4
  _Depends 1.1_
- [x] 2.3 (Unit) Pose HumanEval's and MBPP's tasks in `solution.lot`, and rename the agent bench's files and the names in its `task.toml` and `prompt.md` to `.lot` — R2.3
  _Depends 2.2_
- [x] 2.4 (Unit) Lay the guide's seeded programs, its records' and evaluation's scratch copies and the phase 1 failures it asks about as `.lot` files — R2.5
  _Depends 2.2_
- [x] 2.5 (Unit) Call the project LotML in the README, give the name's origin once, and show `.lot` in its badge and examples — R3.2
