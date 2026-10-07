# Guide tool — design

## What changes

Serves R1.1–R1.4, R2.1–R2.13.

A module `compiler/crates/lotml/src/guide.rs` holding the configuration, the state, the renderer,
the client, the validation and the gate; a `guide` entry in `mcp.rs`'s tool list and dispatch,
present only when configured; and a `guide` subcommand with `ask` and `render`. The agent harness
offers the tool through its own wrapper (specs/agent-harness/, R2.9).

## Boundaries and contracts

**Configuration** (R1.1–R1.4). A TOML file written with the trained model (plans/harness-guide.md
2.4):

```toml
url = "http://127.0.0.1:8088"   # the guide server's OpenAI-compatible endpoint
model = "lotml-guide-0.5b-q4"
threshold = 0.62                # calibrated on the validation split for a target precision
answer = 512                    # the answer's max_tokens
run_candidates = false          # whether an edit for a failing test may be run to check it
renderer = 1                    # taken from the records the guide was trained on
records = "records/<date>"      # relative to this file, or absolute
```

It is found where `LOTML_HARNESS_GUIDE` points, or else as `harness-guide.toml` in the user's lotml
directory — `$XDG_CONFIG_HOME/lotml`, `~/.config/lotml`, or `%APPDATA%\lotml` on Windows — read from
the environment with no new dependency. A relative `LOTML_HARNESS_GUIDE` or `XDG_CONFIG_HOME` is
refused: a harness starts the server in the project, so a relative value would let a project supply
its own guide. Not in the harnesses' configurations either: `lotml init` writes those to be
committed and shared, never with this machine's paths (specs/agent-guide/), and a guide is a model
file and a server on one machine. Found by the server itself, it reaches every harness `init`
registers with nothing added to them. The file is read once at start; an unknown key, a missing
one or a mistyped one removes the tool and says why.

The URL must be exactly `http://` + host + `:` + port, with an optional trailing `/`: host one of
`127.0.0.1`, `[::1]`, `localhost`; port 1–65535 in decimal digits. Anything else is refused —
userinfo (`127.0.0.1@evil`), `\`, `%`, whitespace or control characters, a path, query or fragment,
a suffix (`localhost.evil`), and other spellings of a loopback address (`127.1`, `0x7f.0.0.1`,
`[::ffff:127.0.0.1]`, zone ids). `localhost` is resolved once at start, every address it gives must
be `is_loopback()`, and the tool connects to that address. The state is the user's code and the
tool runs when an agent decides, so a remote URL would send code off the machine unasked; a local
server also needs no TLS, so the client is the standard library's `TcpStream`. The tests are a
table of these forms. `renderer` guards train/serve skew: a guide trained on records rendered one
way and shown a prompt rendered another fails silently, so a mismatch removes the tool.

**Input.** The MCP tool's input schema: `{"paths": [string], "task": string}`, both optional,
`task` at most 2,000 characters. `paths` are resolved as `check`'s are, inside the project; a
symbolic link is skipped. `lotml guide ask [--root DIR] [--task TEXT] [--] FILES…` takes the same
and prints the answer; it exits 0 with an answer or a silence, 1 when no configuration is usable.

**State** (R2.1, R2.2), and the JSON `lotml guide render <state.json>` reads:

```json
{"task": "Write median…" , "path": "stats.lotml", "text": "fn median(…",
 "diagnostics": [{"code": "E0204", "message": "…", "span": {…}, "…": "…"}],
 "failing": null}
```

`diagnostics` holds the file's diagnostics as `lotml check --json` lists them, root cause first;
`failing` one failed block as `lotml test --json` reports it, with the values each side had;
exactly one of the two is set. The tool builds it from `check`'s and `test`'s own paths, the tests
under the call's deadline; running them runs the project's code with the user's privileges, as
`test` does, and the tool's description says so in the same words. The records builder writes the
same JSON from a repair record (specs/guide-records/), and `render` prints
`{"messages": [system, user]}`.

**Rendering** (R2.3). One function writes the messages: a fixed system message naming the answer's
fields and the kinds, then the user turn — the task if given, the file with each line numbered as
`  12 | …`, then the diagnostics as code, message and span, or the failing block with its values.
`RENDERER` is a constant beside it; any change to either text bumps it, and the records must be
rebuilt.

**Asking** (R2.4–R2.6). The body holds the messages, `temperature: 0`, `max_tokens` from
`answer`, `response_format` of type `json_schema` with the answer schema, `logprobs: true` and
`top_logprobs: 1` — the shape llama.cpp's `llama-server`, the runtime the plan names first,
accepts, and that vLLM and Ollama also serve. Without `top_logprobs`, llama-server returns twenty
alternatives per token, some 2 KB each, and a 1024-token answer overruns the body's cap; with `0`
it returns no log-probabilities at all. The schema goes in as text rather than through
`serde_json`, which sorts an object's keys: llama-server compiles the schema to a grammar that
writes the properties in the order they come, so a sorted schema makes the guide write `edit`
before `locations`, out of the order it was trained in, and it degenerates until `max_tokens`. The request carries a fixed `Host`,
`Content-Length`, `Connection: close` and `Accept-Encoding: identity`; the response is read with
the headers capped at 16 KiB and 64 lines, the body at 1 MiB by `Content-Length` or by a chunked
decoder held to the same cap. A status other than 200 is a silence — no redirect is followed. The
server counts the prompt with the model's own tokenizer and refuses one longer than its context;
that refusal is a silence too, and the records builder drops the same records by the same rule
(specs/guide-records/). A rendered state over 256 KiB is not sent.

**One deadline** (R2.5). The call takes `Instant::now() + 75 s` and every step reads the time left:
the state's tests, connecting, each read — its timeout set to what remains, so a server sending a
byte at a time still stops — and the candidate's tests. 75 s keeps the call inside the agent
harness client's 90 s; the MCP server answers one call at a time, so a call that hung would hold
every other tool.

**The answer** — validated before anything uses it (R2.7, R2.8). The JSON is read into closed
structs, unknown fields refused: one to three locations, each `path` (≤ 512 bytes), `symbol` (≤ 256
bytes, or null for a place before the file's first declaration) and `lines` `[start, end]` with
`1 ≤ start ≤ end ≤` the file's line count; `kind` one of `arm`, `body`, `definition`, `add`,
`remove`, `lines`, `several`; `edit` null or one call of `replace`, `add`, `remove` or `edit` with
that tool's own argument struct and `text` ≤ 16 KiB. A `path` must equal one of the project's file
keys exactly — never joined to the root — and a `symbol` must be declared in that file by
`Workspace::outline`; locations failing either are dropped. An edit whose `path` is not a kept
location's is dropped. Confidence is the product of the probabilities of the tokens from the start
of the answer through the first location's symbol, the decision the guide is judged on, and the
threshold applies to it. The edit the agent is shown is serialized again from the struct, so what
was checked is what is shown.

**The gate** (R2.9, R2.10). The edit is applied in memory with `edit::replace`, `add`, `remove` or
`search_replace`, as the MCP tools apply it. The text of every test block (`Kind::Test` in the
outline) must be unchanged — a fix that rewrites the failing test is not a fix — and the file must
check with no error. For a failing block, the candidate is run only when `run_candidates` is true
and the project resolves no interface: lotml without an interface reaches no module but `math` and
its prelude has no I/O beyond `print`, so the candidate can only compute, and the limits below
bound that. The run uses `exec::Scratch` — a fresh directory removed when dropped — holding the
project's `.lotml` files that are not symbolic links, at most 4 MiB in all, the edited one
replaced, and an empty `.git` directory at its root so the interface search stops inside the copy
and never reads a planted `bindings/` above it. `exec::test_report` runs there with the time left
and the `test` tool's output cap; on Unix the runtime sets an address-space limit before running.
An edit failing any step is withheld with its reason; the locations still stand. The project's
files are only read (R2.11).

**The answer shown** (R2.12):

```json
{"locations": [{"path": "stats.lotml", "symbol": "median", "lines": [3, 14]}], "kind": "body",
 "edit": {"tool": "replace", "arguments": {"symbol": "median", "part": "body", "text": "…", "path": "stats.lotml"}},
 "confidence": 0.81, "withheld": null}
```

or `{"guidance": null, "reason": "not-confident"}`. Reasons are a fixed list — `nothing-to-guide`,
`not-confident`, `too-long`, `server-error`, `deadline`, `invalid-answer`, `no-location`, and for a
withheld edit `edit-fails-check`, `edit-changes-tests`, `edit-fails-test`, `not-run` — and no text
of the server's response other than the validated fields reaches the agent. The description says
the guide points and may propose, and the agent decides and edits.

## Alternatives considered

- A separate MCP server for the guide, in Python beside the model: a second registration in every
  harness, and the gate would re-implement `check`'s edits outside the compiler.
- An HTTP client crate (`ureq`, `reqwest`): needed for TLS to a remote server, which R1.4 rules out;
  one POST to a local port is a few dozen lines on `std::net`.
- The model's own stated confidence as the threshold's input: a number the model writes is text
  it was trained to produce, not a probability; the tokens' log-probabilities are.
- Running every candidate, interfaces or not: an interface binding `os` or `subprocess` would let a
  model-written edit run commands nobody reviewed.

## Risks

- `logprobs` with a JSON schema is the runtime's feature, not the protocol's guarantee; the ADR's
  pilot (plans/harness-guide.md 2.1) checks the chosen runtime returns them under a schema.
- Loopback is not trust: an `ssh -L` tunnel or another local user's listener can answer on the
  port, which is why the answer is validated as untrusted input.
- An edit's `text` can carry comments worded as instructions; the agent reads it as data, and the
  description says the agent decides.
- On Windows nothing but the deadline bounds the candidate's memory; a job object would, if a
  candidate is seen to exhaust it.
- Running the tests inside the tool doubles a test run when the agent has just run them; the state
  could take the last `test` report instead if latency shows it matters.
