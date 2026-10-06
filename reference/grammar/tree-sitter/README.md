# lotml for tree-sitter

The grammar editors highlight lotml with. `grammar.js` and `queries/highlights.scm` are generated
from the same source as the other published grammars (`harness/lotml_harness/lang/grammar.py`,
by `python -m lotml_harness.lang.dialects`); `src/scanner.c`, which makes the layout tokens
NEWLINE, INDENT and DEDENT from indentation, is written by hand.

Build the parser with tree-sitter's CLI:

    npx tree-sitter-cli generate
    npx tree-sitter-cli parse program.lotml

`generate` writes `src/parser.c`, which is not kept in the repository.
