## lotml

This project is written in lotml (`.lot`, or `.lotml`): Python's syntax, static types, errors as
values, value semantics. It is not Python — code that looks right in Python is often a compile
error here.

**Learn the language from `lotml.guide.lot`**, before writing any: every rule as code that
checks, with tests. Lines marked `NOT PYTHON` are the traps. Copy its patterns.

### The traps, in short

- Reassigning or mutating needs `var` at the first assignment: `var n = 0`, then `n += 1`.
- No truthiness: `if len(xs) > 0`, `if x is not None`, never `if xs` / `if x`.
- No exceptions: `-> T ! E`, `fail E`, `expr?`, `x ?? default`, `x ?? fail E`; no `try`/`raise`.
- `T?` is the only type admitting `None`; `find`, `get`, `pop`, `to_int` return one.
- Collections and records are values: assigning copies. A function changes its caller's
  value only through `inout` and `&x`.
- `match` must cover every variant. `fn`, not `def`; `type`, not `class`; `impl` for methods.
- No implicit conversions but `/` (always f64): `float(n) + 1.0`. `int` overflow stops the run.
- A project's files do not import each other; only `math`, bound Python modules and C libraries.

### The loop: let the compiler drive

1. Before reading files, get the index: `digest` (signatures and types, no bodies); then `show`
   the symbol you need. Read whole files only to edit prose around code.
2. Edit by symbol: `replace` a definition, body or `match` arm; `add`; `remove`; `rename`.
   Each refuses an edit that breaks the syntax and reports the errors it introduced.
3. `check` after every edit. Diagnostics come root cause first, with the names in scope and
   fixes: apply the fix it gives instead of guessing. `explain E0204` for any code you do not know.
4. `test` runs the `test` blocks; a failure shows the values on each side. Write `test` blocks
   next to the code for every behaviour you add or fix.
5. Done means `check` reports no errors and `test` passes — not that the code looks right.

### Tools

The compiler serves these over MCP (`lotml mcp`, registered by `lotml init`):

| question | tool |
|---|---|
| What is in this project? | `digest` |
| What does this symbol say, and use? | `show` |
| Who uses it? Where is it declared? Its type? | `references`, `definition`, `hover` |
| Is it right? | `check`, then `explain <code>` |
| Does it work? | `test` |
| Change it | `replace`, `add`, `remove`, `edit`, `rename` |

Without MCP, the same from a shell: `lotml check --json <files>`, `lotml check --fix`,
`lotml explain <code>`, `lotml digest .`, `lotml show <symbol> .`, `lotml test --json .`,
`lotml fmt .`, `lotml run main.lot`. `lotml lsp` serves editors.
