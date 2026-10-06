# Guide tool — tasks

## 1 · Configuration

- [ ] 1.1 (TDD) Find the configuration where an absolute `LOTML_HARNESS_GUIDE` points or in the user's lotml directory, refusing relative locations and unknown keys, and list and name `guide` only when it is valid, saying why on standard error when it is not — R1.1, R1.2, R1.3
- [ ] 1.2 (TDD) Accept only `http://` to `127.0.0.1`, `[::1]` or a `localhost` that resolves to loopback alone, with a decimal port and nothing after it, a table of rejected forms in the tests, and refuse another renderer version — R1.4
  _Depends 1.1_

## 2 · The call

- [ ] 2.1 (Unit) Take the state from `check`'s diagnostics, or else the first failing test block with its values, skipping symbolic links, and answer that there is nothing to guide when everything is green — R2.1, R2.2
  _Depends 1.2_
- [ ] 2.2 (Unit) Render the system message and the state with one renderer and its version constant, and print it with `lotml guide render` for a state given as JSON — R2.3, R2.13
  _Depends 2.1_
- [ ] 2.3 (TDD) Ask the guide server in one bounded HTTP exchange under one deadline for the whole call: a body built with `serde_json`, capped headers and body, a bounded chunked decoder, no redirect, silence on any status but 200 — R2.4, R2.5, R2.6
  _Depends 2.2_
- [ ] 2.4 (TDD) Read the answer into closed structs with capped strings and checked line spans, keep only locations in the project's own files and declared symbols and an edit on a kept file, and answer nothing below the threshold — R2.7, R2.8
  _Depends 2.3_
- [ ] 2.5 (TDD) Show an edit only when it leaves every test block's text unchanged and checks clean, and for a failing block passes it in a scratch copy with an empty `.git`, no symbolic links and no interfaces, run only when candidates may run and the project binds nothing, never writing the project — R2.9, R2.10, R2.11
  _Depends 2.4_
- [ ] 2.6 (Unit) Answer as JSON with reasons from the fixed list, through the MCP tool and through `lotml guide ask` — R2.12, R2.13
  _Depends 2.5_
