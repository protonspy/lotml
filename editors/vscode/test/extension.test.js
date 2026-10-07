// The extension against stand-ins for VS Code, the language client and the process it probes:
// `node --test test/extension.test.js` (specs/file-icons/ R3.2-R3.4).

const assert = require("node:assert");
const fs = require("node:fs");
const Module = require("node:module");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");

const EXE = process.platform === "win32" ? "lotml.exe" : "lotml";

/** extension.js loaded with `setting` as lotml.path and `probe` as the compiler's run. */
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
      execFile: (command, args, options, done) => {
        seen.probed.push([command, args]);
        done(probe.error ?? null, "lotml 0.1.0\n", "");
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

const made = [];
test.after(() => {
  for (const each of made) {
    fs.rmSync(each, { recursive: true, force: true });
  }
});

/** A fresh directory, holding a compiler named `name` when one is given. */
function directory(name) {
  const fresh = fs.mkdtempSync(path.join(os.tmpdir(), "lotml-ext-"));
  made.push(fresh);
  if (name) {
    fs.writeFileSync(path.join(fresh, name), "");
  }
  return fresh;
}

/** Run `body` with the PATH holding `directories` only. */
async function withPath(directories, body) {
  const saved = process.env.PATH;
  process.env.PATH = directories.join(path.delimiter);
  try {
    await body();
  } finally {
    process.env.PATH = saved;
  }
}

test("starts `lotml lsp` from the PATH for LotML files by default", async () => {
  const bin = directory(EXE);
  await withPath([bin], async () => {
    const { extension, seen } = load(undefined, {});
    await extension.activate();
    const compiler = path.join(bin, EXE);
    assert.deepStrictEqual(seen.probed, [[compiler, ["--version"]]]);
    assert.strictEqual(seen.clients.length, 1);
    const [client] = seen.clients;
    assert.deepStrictEqual(client.server, { command: compiler, args: ["lsp"] });
    assert.deepStrictEqual(client.options.documentSelector, [
      { scheme: "file", language: "lotml" },
    ]);
    assert.ok(client.started);
    assert.deepStrictEqual(seen.shown, []);
    await extension.deactivate();
    assert.strictEqual(seen.stopped, 1);
  });
});

test("starts the compiler lotml.path names", async () => {
  const { extension, seen } = load("C:/tools/lotml.exe", {});
  await extension.activate();
  assert.strictEqual(seen.clients[0].server.command, "C:/tools/lotml.exe");
});

test("never runs a compiler the current directory holds but the PATH does not", async () => {
  const planted = directory(EXE);
  const saved = process.cwd();
  process.chdir(planted);
  try {
    await withPath([directory()], async () => {
      const { extension, seen } = load(undefined, {});
      await extension.activate();
      assert.deepStrictEqual(seen.probed, []);
      assert.strictEqual(seen.clients.length, 0);
      assert.match(seen.shown[0], /not found on the PATH/);
    });
  } finally {
    process.chdir(saved);
  }
});

test("says once, naming the setting, when the compiler cannot run", async () => {
  const { extension, seen } = load("/opt/lotml", { error: new Error("spawn EACCES") });
  await extension.activate();
  assert.strictEqual(seen.shown.length, 1);
  assert.match(seen.shown[0], /cannot run `\/opt\/lotml` \(spawn EACCES\).*lotml\.path/);
  assert.strictEqual(seen.clients.length, 0, "no client, so no second message");
  await extension.deactivate();
  assert.strictEqual(seen.stopped, 0);
});
