# Agent harness

A deepagents agent on the agent benchmark (`harness/agent_bench/`), with the compiler's
MCP tools, graded on hidden tests (specs/agent-harness/). Arm `agents`: `lotml init`
wrote `AGENTS.md`, loaded as the agent's memory, and `lotml.guide.lotml`; arm
`reference`: the language reference in the system prompt. Written by
`python -m lotml_harness.agent`.

## Results

| model | arm | tasks | runs | pass@1 | 95% interval | pass@k | hidden tests | checks | errors | stopped |
| --- | --- | ---: | ---: | ---: | --- | --- | ---: | ---: | ---: | --- |
| z-ai/glm-5.3-flash | agents | 1 | 1 | 1.00 | 0.21-1.00 | - | 100% | 1/1 | 0 | done 1 |

## Per run, on average

| model | arm | model calls | tool calls | tool errors | checks with errors | input tokens | output tokens | reasoning tokens | cost (USD) | seconds | lines changed |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| z-ai/glm-5.3-flash | agents | 13.0 | 14.0 | 0.0 | 0.0 | 142,182 | 3,256 | 2,290 | 0.0046 | 136 | 27.0 |

## Per task

Runs passed out of runs graded.

| task | kind | z-ai/glm-5.3-flash agents |
| --- | --- | ---: |
| median-mode | implement | 1/1 |

Eight tasks are far below the 168 paired tasks a 10-point difference needs
(docs/wiki/pages/evaluation-harness.md): read the arms' difference as a direction, and
the per-task table and the traces in `harness/cache/agent/` for where the agent fails.
