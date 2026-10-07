# File icons — design

## What changes

Serves R1.1, R1.2, R2.1–R2.3, R3.1–R3.4.

**The icon** (R1.1, R1.2). `python -m lotml_harness.icons` draws it, the way
`python -m lotml_harness.lang.dialects` writes the published grammars. Each size small enough to
blur is drawn as its own pixel grid: 16 and 32 pixels, one character per pixel and a palette of the
project's colours. The grid shows a page with a folded corner and the lotus on it, amber petals and
teal leaves. 48 is the 16 grid tripled and 256 the 32 grid multiplied by eight, so every size stays
pixel art with no smoothing. The generator writes PNG with `zlib` and `struct` from the standard
library, and the `.ico` as a directory of those four PNGs, which Windows reads from Vista on. The
images go to `editors/icons/`, and the 32-pixel one is copied into the extension, since a VS Code
extension can only ship files under its own directory. The generated files are committed, and a
test regenerates them and compares them byte for byte.

**Windows Explorer** (R2.1–R2.3). `editors/windows/register.ps1`, in Windows PowerShell 5.1,
writes under `HKCU\Software\Classes` only, so no administrator is needed:

- `.lot` and `.lotml` get the default value `LotML.Source`;
- `LotML.Source` gets the default value `LotML source file`, and its `DefaultIcon` the path of the
  icon copied to `%LOCALAPPDATA%\LotML\lotml-file.ico`.

A copy is used so the icon survives the checkout being moved or deleted. Explorer is told with
`SHChangeNotify(SHCNE_ASSOCCHANGED)`. An extension whose default value already names another type,
under `HKCU` or under `HKLM`, is left alone and the script says which, unless `-Force` is given.
`-Force` records the user's type it displaces as `Displaced<ext>` on `LotML.Source`. `-Remove`
gives each extension still pointing at `LotML.Source` back to its recorded type, or clears its
default value and deletes the key once it is empty, then deletes `LotML.Source` and the copied
icon. `HKLM` is only read. `-Root`, `-MachineRoot` and `-IconHome` move those places for the tests, which run the script on
Windows against a throwaway key.

**VS Code** (R3.1–R3.3). The extension lives in `editors/vscode/`: `package.json`,
`language-configuration.json` and one `extension.js`, plain JavaScript with no build step.
`contributes.languages` declares `lotml`, aliased `LotML`, for `.lot` and `.lotml`, with the icon
for both themes. VS Code shows that icon wherever the file icon theme has none of its own for the
language. `extension.js` starts `vscode-languageclient`'s `LanguageClient` on `<lotml.path> lsp`
over standard input and output, selecting `file` documents of the language. The client shows an
error of its own when the process fails, so the compiler is first run with `--version`, without
blocking: when that fails, the extension shows one error naming the setting and starts no
client (R3.3). The setting's scope is `machine`, so only the user's settings choose the program,
and the manifest declares no support for untrusted workspaces. A bare name is looked up in the
`PATH`'s absolute directories by the extension itself, because a process started by a bare name
on Windows is looked for in the current directory first, which an opened repository could fill
(R3.4). `npm ci` and
`npm run package`, through the pinned `@vscode/vsce`, build `lotml-<version>.vsix`, which is not committed;
`code --install-extension` installs it.

## Alternatives considered

- **Generating the icon with `pixellab-cli`**, as the lotus was. At 16 pixels a generated image
  needs redrawing anyway, and every regeneration costs a paid call. A grid in the repository is
  free, reviewable and exact.
- **A `lotml` subcommand writing the registry.** The binary would gain a Windows-only registry
  crate for something done once per machine. The script needs nothing installed.
- **TypeScript for the extension.** It brings a compile step and a dependency for about forty
  lines.

## Risks

- A user who picked a program with Explorer's "Open with" has a `UserChoice` for the extension,
  and Explorer then takes the icon from that program's type. The script reports such a
  `UserChoice` and leaves it alone: it is protected by a hash Windows computes.
- A file icon theme that maps `.lot` or the `lotml` language to an icon of its own wins over the
  language's icon.
