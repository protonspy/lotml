// The LotML extension: package.json gives LotML files their language and their icon, and this
// starts the compiler's language server for them (specs/file-icons/ R3.2, R3.3).

const { spawnSync } = require("node:child_process");
const vscode = require("vscode");
const { LanguageClient } = require("vscode-languageclient/node");

let client;

/** Start `<lotml.path> lsp`, or say once why it cannot run and leave the language as it is. */
async function activate() {
  const path = vscode.workspace.getConfiguration("lotml").get("path", "lotml");
  // The client shows its own error when the server's process fails, so the compiler is tried
  // first: one message, naming the setting that fixes it.
  const probe = spawnSync(path, ["--version"], { encoding: "utf8", timeout: 10000 });
  if (probe.error || probe.status !== 0) {
    const why = probe.error ? probe.error.message : `exit status ${probe.status}`;
    vscode.window.showErrorMessage(
      `LotML: cannot run \`${path}\` (${why}); set lotml.path to the lotml compiler.`,
    );
    return;
  }
  client = new LanguageClient(
    "lotml",
    "LotML",
    { command: path, args: ["lsp"] },
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
