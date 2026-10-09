# Python re-exports — tasks

- [ ] 1.1 (Unit) Find the modules a stub re-exports from, breadth first, each once, within depth 4 and 8 MiB — R1.3, R1.6
- [ ] 1.2 (Unit) Bind a re-exported name as the stub's own: named, starred, renamed, followed through its source, the reason listed when its source has no stub — R1.1, R1.2, R1.3, R1.4, R1.5
  _Depends 1.1_
- [ ] 2.1 (Unit) Find a stub's parts as its own is found, and key the cache and the lock by them — R2.1, R2.3
  _Depends 1.2_
- [ ] 2.2 (Unit) Read a module of a given stub's package from beside it in `lotml bind --stub` — R2.2
  _Depends 2.1_
- [ ] 3.1 (Unit) Measure the corpus as `reexports` beside `generics`, and the bind-on-import report again — R3.1, R3.2
  _Depends 2.2_
- [ ] 3.2 (Unit) Write re-exports into the wiki and the notes — R1.1, R2.1
  _Depends 3.1_
