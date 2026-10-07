---
autonomy: auto
ci: wait
branch: feat/file-icons
delivery: in-progress
---

# File icons — requirements

## Purpose

A LotML file should be recognisable at a glance, the way a Python file is: the lotus on a page,
in the project's teal and amber, beside every `.lot` and `.lotml` file in Windows Explorer and in
VS Code. Explorer takes an icon from a per-user file type, and VS Code from an extension that
declares the language. The extension also starts the compiler's language server, so the files
the icon marks get their diagnostics, definitions and fixes in the editor.

## R1 · The icon

- **R1.1** The repository shall hold the file icon, the lotus on a page, as PNG images of 16, 32, 48 and 256 pixels and as one Windows icon file holding all four sizes.
- **R1.2** The icon generator shall draw the 16- and 32-pixel images from their own pixel grids, and derive the 48- and 256-pixel images by whole-number scaling of those grids.

## R2 · Windows Explorer

- **R2.1** When the user runs the Windows registration script, the script shall associate `.lot` and `.lotml` with a LotML file type for the current user only, whose icon is a copy of the icon file under the user's local application data, and shall refresh Explorer's icons.
- **R2.2** If an extension is already associated with a file type other than LotML's, for the user or for the machine, then the script shall leave that association as it is and say so, unless it is run with `-Force`, which records the type it displaces.
- **R2.3** When the script runs with `-Remove`, the script shall delete the keys and the icon copy it wrote, give a displaced extension back to its type, and change nothing else.

## R3 · VS Code

- **R3.1** The VS Code extension shall declare the language LotML for `.lot` and `.lotml` files, with the file icon for light and dark themes, `#` line comments, the bracket pairs and an indent after a line ending in `:`.
- **R3.2** When a LotML file is opened, the extension shall start `lotml lsp` from the `lotml.path` setting, `lotml` by default, for the workspace's LotML files.
- **R3.3** If the language server cannot be started, then the extension shall say so once and keep the language and the icon.
- **R3.4** The extension shall read `lotml.path` from the user's settings only, look a bare name up in the `PATH`'s directories only, and stay inactive in a workspace VS Code does not trust.

## Out of scope

- Syntax highlighting in VS Code: it needs a TextMate grammar, which the published grammars do
  not include yet.
- Publishing the extension to the Marketplace; it is built as a `.vsix` and installed from it.
- PyCharm and the other JetBrains IDEs, GitHub's Linguist, macOS and Linux file managers.
- Opening `.lot` files with a program from Explorer: the type carries an icon, not a command.
