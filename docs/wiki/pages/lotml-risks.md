# lotml risks

The original study's risk table, revised with what was measured and researched. Three risks
changed weight and five are new. Each row points to the page that backs the assessment.

## The original study's risks, revised

| risk | revised impact | what changed | mitigation |
| --- | --- | --- | --- |
| no training corpus | high for open models; medium for frontier models with the spec | the pilot found no syntactic leakage in 60 programs; leakage moved to semantics ([[python-leakage-pilot]]) | short spec plus compiler tools; corpus from validated translation; RL with verifiable rewards ([[training-prior]]) |
| empty ecosystem | high | no language generates typed bindings from `.pyi` stubs; stubs do not declare exceptions | Python target; bindings generated from typeshed, with every call into Python returning `T ! PyError` ([[transpilation-strategy]]) |
| tokenizer dependence | **low** (was medium) | token ratios varied by less than 2 percentage points across eight tokenizers ([[token-cost]]) | measuring on several tokenizers stays cheap |
| conciseness versus readability | medium | no change | principle 8; canonical formatter |
| models evolve fast | medium | reinforced: agentic cost is dominated by repair rounds, not surface tokens | bet on correctness and on the [[semantic-compiler]] |
| compile time | medium | the incremental frontend weighs more than the backend (Roc: 35 ms; Cranelift: ~5% of a full rustc build) | query-based architecture from the start |
| project scope | high | no change | small MVP; measured gates ([[evaluation-harness]]) |
| errors reported in generated code | high | precedents confirmed: Python AST with positions, `#line` in C | no error in terms of generated code |
| badly generated concurrency | medium, **not measured** | no source measures "forgot the `await`"; colorless runtimes leak at blocking FFI | colorless concurrency; blocking FFI handed to dedicated threads ([[colorless-concurrency]]) |
| reference-counting cost | **medium** (was low/medium) | atomic counting cost 5–59% in Perceus; Swift spent 32% of its time counting | non-atomic counting within a task; copy or move between tasks ([[memory-model]]) |

## New risks

| risk | impact | evidence | mitigation |
| --- | --- | --- | --- |
| semantic Python leakage | high | the parser accepts mutating an immutable, truthiness and arguments treated as references; the pilot found all three | dedicated diagnostics with applicable fixes; variant B; explicit `inout` convention |
| incomplete grammar | high, if used to constrain generation | an incomplete constrainer cut correctness by up to 97%; the pilot's grammar rejected two valid programs | grammar generated from the same source as the parser and tested against the corpus ([[constrained-decoding]]) |
| indentation in agent edits | medium, not measured | SWE-agent and aider needed guards against indentation errors while editing | editing test in the harness before v1 freezes ([[lotml-syntax]]) |
| unreachable token target | medium | variant A gives about 10% fewer tokens than typed Python; the target was 20% | replace the target with "no worse than typed Python" ([[requirements-and-roadmap]]) |
| biased evaluation | medium | the paired corpus has a single author; the pilot used one model family | external tasks, several families, multiple samples |
