# Seeded failures — tasks

## 1 · The mutation command

- [ ] 1.1 (TDD) List the mutants of the families that break checking — names, types, calls, mutability — each a single span replaced, with its operator and declaration — R2.1, R2.2
- [ ] 1.2 (TDD) List the mutants that keep a program checking but change its meaning — operators, bounds, swapped arguments, negated conditions, constants, dropped statements — R2.1, R2.2
  _Depends 1.1_
- [ ] 1.3 (Unit) Print them as `lotml dev mutate <file> --json`, deterministic and in source order — R2.1
  _Depends 1.2_

## 2 · The programs

- [ ] 2.1 (Unit) Take the benchmark's solutions with their hidden blocks, and the final files of passing runs the licence registry permits, from train and validation problems only, a held-out one stopping the run — R1.1, R3.2
- [ ] 2.2 (Unit) Translate HumanEval's canonical solutions by the rules alone, typed from the witness, keeping those that check and pass their hidden blocks and counting the rest by reason — R1.2, R1.3

## 3 · Drawing, judging, writing

- [ ] 3.1 (Unit) Weight the families from the committed reports' error counts and draw up to the cap per program and family with a seeded generator, each identical mutant once per problem — R2.5, R2.6
  _Depends 1.3_
- [ ] 3.2 (Unit) Judge each mutant in a scratch copy — refused by `check`, or a test failing — dropping the equivalent and those `check --fix` makes clean — R2.3, R2.4
  _Depends 3.1_
- [ ] 3.3 (Unit) Write the kept mutants as repair records marked seeded, and the committed report with every count by operator and reason — R3.1, R3.3
  _Depends 2.1, 2.2, 3.2_
