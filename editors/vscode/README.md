# LotML for VS Code

Gives `.lot` and `.lotml` files the LotML language and its lotus icon, and starts the compiler's
language server for them: diagnostics with their fixes, definitions, references, hover, rename,
symbols and formatting.

The icon shows wherever the file icon theme has none of its own for the language. Syntax
highlighting is not included yet.

## Install

Build it from this directory, then install the file it writes:

    npm ci
    npm run package
    code --install-extension lotml-0.1.0.vsix

The language server is `lotml lsp`. Set `lotml.path` when `lotml` is not on the `PATH`, for example
to `compiler/target/release/lotml.exe` in a checkout of the repository.
