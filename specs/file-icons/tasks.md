# File icons — tasks

## 1 · The icon

- [x] 1.1 (Unit) Draw the 16- and 32-pixel grids of the lotus on a page, and generate the four PNGs and the `.ico` with `python -m lotml_harness.icons`, committed and checked byte for byte by a test — R1.1, R1.2

## 2 · Windows Explorer

- [x] 2.1 (Unit) Write `editors/windows/register.ps1`: the per-user association of both extensions with the icon's copy, Explorer's refresh, another type's association left unless `-Force`, and `-Remove` — R2.1, R2.2, R2.3
  _Depends 1.1_

## 3 · VS Code

- [x] 3.1 (Unit) Write the extension's manifest and language configuration: LotML for `.lot` and `.lotml`, with the icon, comments, brackets and indentation — R3.1
  _Depends 1.1_
- [x] 3.2 (Unit) Start `lotml lsp` from `lotml.path` with `vscode-languageclient`, report a failed start once, and build the `.vsix` — R3.2, R3.3
  _Depends 3.1_

## 4 · The record

- [x] 4.1 (Unit) List Node, `vscode-languageclient` and `@vscode/vsce` in `docs/stack.md`, and say in the README how to install the icons in Explorer and VS Code — R2.1, R3.2
  _Depends 2.1, 3.2_
