# Guide tool — design

## What changes

Serves R1.1–R1.3, R2.1–R2.11.

A module `compiler/crates/lotml/src/guide.rs` holding the configuration, the state, the renderer,
the client and the gate; a `guide` entry in `mcp.rs`'s tool list and dispatch, present only when
configured; and a `guide` subcommand with `ask` and `render`.

**Configuration** (R1.1–R1.4). A TOML file written with the trained model (plans/harness-guide.md
2.4), found where `LOTML_HARNESS_GUIDE` points or else as `harness-guide.toml` in the user's lotml
directory — `$XDG_CONFIG_HOME/lotml`, `~/.config/lotml`, or `%APPDATA%\lotml` on Windows, from the
environment, with no new dependency. Not in the harnesses' configurations: `lotml init` writes
those to be committed and shared, never with this machine's paths (specs/agent-guide/), and a
guide is a model file and a server on one machine. Found by the server itself, it reaches every
harness `init` registers with nothing added to them; the server's instructions gain a sentence on
`guide` only when it is listed.

```toml
url = "http://127.0.0.1:8088"   # the guide server's OpenAI-compatible endpoint
model = "lotml-guide-0.5b-q4"
threshold = 0.62                # calibrated on the validation split for a target precision
context = 4096                  # tokens, the base model's
seconds = 10
renderer = 1                    # the version of the prompt the records were rendered with
records = "harness/cache/guide/records/<date>"
```

The file is read once at start; relative paths resolve against its directory. A URL is accepted only as `http://` to `127.0.0.1`, `[::1]` or
`localhost` with a port: the state is the user's code and the tool runs when an agent decides, so a
remote URL would send code off the machine unasked — and a local server needs no TLS, so the client
is the standard library's `TcpStream`, one HTTP/1.1 POST with a length and a bounded read, and no
new dependency. `renderer` guards train/serve skew: a guide trained on records rendered one way and
shown a prompt rendered another fails silently, so a mismatch removes the tool and says so. The
`records` path is for the evaluation's leakage guard (specs/guide-evaluation/ R1.3).

**State** (R2.1, R2.2). The tool reuses `check`'s and `test`'s own paths: the selected files'
diagnostics in the order `check` reports them, root cause first; if none, `exec::test_report` under
the `test` tool's limits, the first failing block with the values each side had. Running the tests
runs the project's code with the user's privileges, as `test` does, and the tool's description says
so in the same words.

**Rendering** (R2.3). One function writes the messages: a fixed system message naming the answer's
fields and the kinds, then the user turn — the task if given, the file with each line numbered as
`  12 | …`, then the diagnostics as code, message and span, or the failing block with its values.
`RENDERER` is a constant beside it; any change to either text bumps it, and with it the records
must be rebuilt. `lotml guide render <state.json>` prints the messages as JSON for the records
builder (specs/guide-records/), which is how one renderer serves training and inference.

**Asking** (R2.4–R2.6). `POST /v1/chat/completions` with the messages, `temperature: 0`,
`response_format` of type `json_schema` with the answer schema, `logprobs: true` — the shape
llama.cpp's `llama-server`, the runtime the plan names first, accepts, and that vLLM and Ollama
also serve. The schema:

```json
{"locations": [{"path": "stats.lotml", "symbol": "median", "lines": [3, 14]}],
 "kind": "body",
 "edit": {"tool": "replace", "arguments": {"symbol": "median", "part": "body", "text": "…"}}}
```

`kind` is one of `arm`, `body`, `definition`, `add`, `remove`, `lines`, `several`; `edit` is null
or one call of `replace`, `add`, `remove` or `edit`, the tools the agent already has. Confidence is
the product of the probabilities of the tokens from the start of the answer through the first
location's symbol — the decision the guide is judged on — and the threshold applies to it. Every
failure on the way — a refused connection, a status other than 200, the time limit, a body past
1 MB, JSON outside the schema — is an answer of nothing, with the reason.

**The gate** (R2.7, R2.8). Locations whose symbol `Workspace::outline` does not find in the file are
dropped. An edit is applied in memory with `edit::replace`, `add`, `remove` or `search_replace`, as
the MCP tools apply it; the result must check with no error in that file, and for a failing block
the project is copied to a temporary directory with the edited file, and `exec::test_report` must
pass that block. An edit failing either is withheld and the reason given; the locations still
stand. The project's files are only read (R2.9).

**The answer** (R2.10):

```json
{"locations": [{"path": "stats.lotml", "symbol": "median", "lines": [3, 14]}], "kind": "body",
 "edit": {"tool": "replace", "arguments": {"symbol": "median", "part": "body", "text": "…"}},
 "confidence": 0.81, "withheld": null}
```

or `{"guidance": null, "reason": "not confident (0.41 below 0.62)"}`. The agent reads it as one
opinion: the description says the guide points and may propose, and the agent decides and edits.

## Alternatives considered

- A separate MCP server for the guide, in Python beside the model: a second registration in every
  harness, and the gate would re-implement `check`'s edits outside the compiler.
- An HTTP client crate (`ureq`, `reqwest`): needed for TLS to a remote server, which R1.3 rules out;
  one POST to a local port is a few dozen lines on `std::net`.
- The model's own stated confidence as the threshold's input: a number the model writes is text
  it was trained to produce, not a probability; the tokens' log-probabilities are.

## Risks

- `logprobs` with a JSON schema is the runtime's feature, not the protocol's guarantee; the ADR's
  pilot (plans/harness-guide.md 2.1) checks the chosen runtime returns them under a schema.
- Running the tests inside the tool doubles a test run when the agent has just run them; the state
  could take the last `test` report instead if latency shows it matters.
