# Python overloads — tasks

- [ ] 1.1 (Unit) Carry a signature's overloads in `FnSig` and read a Python interface's repeated function or member as overloads, E0221 for a receiver that differs and past 64 — R2.1, R2.2
- [ ] 1.2 (TDD) Give an overloaded call the first overload its arguments fit, each argument checked once and a `PyObject` fit taken last, E0204 when none fits, and record the choice by the call's span — R3.1, R3.2
  _Depends 1.1_
- [ ] 1.3 (Unit) Report E0226 for an overloaded function named other than to call it — R3.3
  _Depends 1.1_
- [ ] 1.4 (Unit) Lower an overloaded call by the overload the checker recorded, through to the Python target's run — R3.4
  _Depends 1.2_
- [ ] 2.1 (Unit) Bind a stub's overload runs in order, cut at the first that cannot be bound, without repeats, the first branch's, at most 64 — R1.1, R1.2, R1.3, R1.4, R1.5
  _Depends 1.1_
- [ ] 3.1 (Unit) Count an overloaded name typed when one overload is, and measure the corpus as `overloads` beside `classes` — R4.1, R4.2
  _Depends 2.1_
- [ ] 3.2 (Unit) Write overloads into the language reference, the guide, the E0221 and E0226 explanations and the wiki — R2.1, R3.1, R3.3
  _Depends 1.4, 2.1_
