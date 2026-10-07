// The LotML extension: package.json gives LotML files their language and their icon, and this
// starts the compiler's language server for them (specs/file-icons/ R3.2-R3.4).

const { execFile } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");
const { promisify } = require("node:util");
const vscode = require("vscode");
const { LanguageClient } = require("vscode-languageclient/node");

let client;

/**
 * `command` when it names a path, else the first file of that name in the PATH's directories:
 * a process started by a bare name on Windows is looked for in the current directory first,
 * which an opened repository could fill.
 */
function located(command) {
  if (command.includes("/") || command.includes("\\")) {
    return command;
  }
  const names =
    process.platform === "win32" ? [`${command}.exe`, `${command}.com`, command] : [command];
  const directories = (process.env.PATH ?? "").split(path.delimiter).filter(path.isAbsolute);
  for (const directory of directories) {
    for (const name of names) {
      const candidate = path.join(directory, name);
      if (fs.statSync(candidate, { throwIfNoEntry: false })?.isFile()) {
        return candidate;
      }
    }
  }
  return undefined;
}

/** Start `<lotml.path> lsp`, or say once why it cannot run and leave the language as it is. */
async function activate() {
  const setting = vscode.workspace.getConfiguration("lotml").get("path", "lotml");
  const command = located(setting);
  // The client shows its own error when the server's process fails, so the compiler is tried
  // first: one message, naming the setting that fixes it.
  try {
    if (command === undefined) {
      throw new Error("not found on the PATH");
    }
    await promisify(execFile)(command, ["--version"], { timeout: 10000 });
  } catch (error) {
    vscode.window.showErrorMessage(
      `LotML: cannot run \`${setting}\` (${error.message}); set lotml.path, in your user ` +
        "settings, to the lotml compiler.",
    );
    return;
  }
  client = new LanguageClient(
    "lotml",
    "LotML",
    { command, args: ["lsp"] },
    { documentSelector: [{ scheme: "file", language: "lotml" }] },
  );
  await client.start();
}

/** Stop the language server, when one was started. */
async function deactivate() {
  if (client) {
    await client.stop();
    client = undefined;
  }
}

module.exports = { activate, deactivate };
