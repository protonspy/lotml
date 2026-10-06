# Guide tool — tasks

## 1 · Configuration

- [ ] 1.1 (TDD) Find the configuration where `LOTML_HARNESS_GUIDE` points or in the user's lotml directory, accept only plain HTTP on the loopback interface and the compiler's renderer version, and list and name `guide` only when it is valid, saying why on standard error when it is not — R1.1, R1.2, R1.3, R1.4

## 2 · The call

- [ ] 2.1 (Unit) Take the state from `check`'s diagnostics, or else the first failing test block with its values, and answer that there is nothing to guide when everything is green — R2.1, R2.2
  _Depends 1.1_
- [ ] 2.2 (Unit) Render the system message and the state with one renderer and its version constant, printed by `lotml guide render` for a state given as JSON — R2.3, R2.11
  _Depends 2.1_
- [ ] 2.3 (Unit) Ask the guide server in one HTTP POST held to the answer schema with log-probabilities, answering nothing on a refusal, a timeout, an oversized or malformed answer, or a state over the context budget — R2.4, R2.5
  _Depends 2.2_
- [ ] 2.4 (TDD) Take the confidence from the probabilities of the tokens through the first location's symbol, and answer nothing below the threshold — R2.6
  _Depends 2.3_
- [ ] 2.5 (TDD) Drop locations the file does not declare, and show an edit only when, applied to a copy, it checks clean and passes the failing block, never writing the project — R2.7, R2.8, R2.9
  _Depends 2.4_
- [ ] 2.6 (Unit) Answer as JSON through the MCP tool and through `lotml guide ask` — R2.10, R2.11
  _Depends 2.5_
