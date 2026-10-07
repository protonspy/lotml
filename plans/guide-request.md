---
autonomy: auto
ci: wait
---

# Guide request

The `guide` tool's request to llama-server, fixed so the tuned guide can answer through it: the
answer schema sent in the order the guide was trained to answer in, and one log-probability
alternative per token.

## Why

Asked through `lotml guide ask`, the tuned guide was silent with `server-error` on every real
failure tried. serde_json sorted the schema's keys, so llama.cpp's grammar made the guide write
`edit` before `locations`; out of its training it degenerated until `max_tokens`. llama-server also
returned twenty alternatives per token, some 2 KB each, so a long answer overran the tool's 1 MiB
body cap. Done when the tool sends the schema's keys in the trained order with `top_logprobs: 1`,
and the tuned guide answers a real held-out failure through it.

## Paths

- `compiler/crates/lotml/src/guide/client.rs`
- `specs/guide-tool/design.md`

## References

- `specs/guide-tool/` — the tool whose request this fixes (R2.4, R2.6)
- adr:0017-a-half-billion-coder-model-tuned-locally-and-served-by-llama-server — the runtime

## Tasks

- [x] 1.1 (Unit) Send the answer schema as text in the trained key order, and ask for one log-probability alternative per token

## Done when

- `cargo test -p lotml --bin lotml guide::client` passes, its request test checking the key order and `top_logprobs`
- `lotml guide ask` answers a held-out phase 1 failure from the tuned guide on llama-server b11450
