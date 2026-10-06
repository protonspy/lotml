# Agent harness — tasks

## 1 · Dependencies and the benchmark

- [x] 1.1 (Unit) Add deepagents and langchain-openrouter to the harness, with the ADR and the stack entries — R2.1
- [x] 1.2 (Unit) Load benchmark tasks and grade a workspace: check, hidden tests appended per graded file — R1.1, R3.1, R3.2
- [x] 1.3 (Unit) Write eight benchmark tasks with reference solutions, and a test that every solution passes and every starting workspace fails — R1.1, R1.2, R1.3
  _Depends 1.2_

## 2 · The agent

- [ ] 2.1 (Unit) Speak to `lotml mcp` over stdio and turn its tools into LangChain tools, the server started without the key — R2.3
- [ ] 2.2 (Unit) Run one task: workspace copy, the arm's context, the agent on a confined filesystem with no shell, step and time limits, model errors recorded, the key required — R2.1, R2.2, R2.4, R2.5, R2.6, R2.7, R2.8
  _Depends 1.1, 2.1_
- [ ] 2.3 (Unit) Read a run's metrics from its messages and keep its trace in the cache — R4.1, R4.4
  _Depends 2.2_

## 3 · The report

- [ ] 3.1 (TDD) Compute pass@k by the unbiased estimator and the Wilson interval — R4.2
- [ ] 3.2 (Unit) Run the benchmark from the command line, skipping runs already recorded, and write the report — R4.2, R4.3
  _Depends 2.3, 3.1_
- [ ] 3.3 (Unit) Run the benchmark on `z-ai/glm-5.3-flash` in both arms and commit the rows and the report — R4.1, R4.2
  _Depends 3.2_
