// The extension against stand-ins for VS Code, the language client and the process it probes:
// `node --test test/*.test.js` (specs/file-icons/ R3.2, R3.3).

const assert = require("node:assert");
const Module = require("node:module");
const path = require("node:path");
const test = require("node:test");

/** extension.js loaded with `setting` as lotml.path and `probe` answering the compiler's run. */
function load(setting, probe) {
  const seen = { shown: [], clients: [], stopped: 0, probed: [] };
  class LanguageClient {
    constructor(id, name, server, options) {
      Object.assign(this, { id, name, server, options });
      seen.clients.push(this);
    }
    async start() {
      this.started = true;
    }
    async stop() {
      seen.stopped += 1;
    }
  }
  const stubs = {
    vscode: {
      workspace: {
        getConfiguration: (section) => ({
          get: (key, fallback) =>
            section === "lotml" && key === "path" && setting !== undefined ? setting : fallback,
        }),
      },
      window: { showErrorMessage: (message) => seen.shown.push(message) },
    },
    "vscode-languageclient/node": { LanguageClient },
    "node:child_process": {
      spawnSync: (command, args) => {
        seen.probed.push([command, args]);
        return probe;
      },
    },
  };
  const original = Module._load;
  Module._load = function (request, ...rest) {
    return request in stubs ? stubs[request] : original.call(this, request, ...rest);
  };
  const file = path.join(__dirname, "..", "extension.js");
  delete require.cache[file];
  try {
    return { extension: require(file), seen };
  } finally {
    Module._load = original;
  }
}

test("starts `lotml lsp` for LotML files by default", async () => {
  const { extension, seen } = load(undefined, { status: 0 });
  await extension.activate();
  assert.deepStrictEqual(seen.probed, [["lotml", ["--version"]]]);
  assert.strictEqual(seen.clients.length, 1);
  const [client] = seen.clients;
  assert.deepStrictEqual(client.server, { command: "lotml", args: ["lsp"] });
  assert.deepStrictEqual(client.options.documentSelector, [{ scheme: "file", language: "lotml" }]);
  assert.ok(client.started);
  assert.deepStrictEqual(seen.shown, []);
  await extension.deactivate();
  assert.strictEqual(seen.stopped, 1);
});

test("starts the compiler lotml.path names", async () => {
  const { extension, seen } = load("C:/tools/lotml.exe", { status: 0 });
  await extension.activate();
  assert.strictEqual(seen.clients[0].server.command, "C:/tools/lotml.exe");
});

test("says once, naming the setting, when the compiler cannot run", async () => {
  for (const probe of [{ error: new Error("spawn lotml ENOENT") }, { status: 2 }]) {
    const { extension, seen } = load("lotml", probe);
    await extension.activate();
    assert.strictEqual(seen.shown.length, 1);
    assert.match(seen.shown[0], /cannot run `lotml`.*lotml\.path/);
    assert.strictEqual(seen.clients.length, 0, "no client, so no second message");
    await extension.deactivate();
    assert.strictEqual(seen.stopped, 0);
  }
});
