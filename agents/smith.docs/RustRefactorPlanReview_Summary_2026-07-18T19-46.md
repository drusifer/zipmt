# Rust Refactor Plan — Combined User Review

**Verdict:** Approved

The five stories and architecture are maintenance-focused and do not impose a
new user mental model. The plan explicitly preserves CLI/TUI surfaces, output
compatibility, progress visibility, cancellation, cleanup, diagnostics, live
controls, terminal layouts, and real-PTY completion.

The acceptance criteria are observable and blocking: decompression
equivalence, bounded RSS, output ordering, snapshot/PTY behavior, error and
cleanup compatibility, and a measured 5% throughput budget.

Relevant HCI protections:

- Heuristic 1, system status: TUI state, telemetry, and completion behavior are
  characterization gates.
- Heuristic 3, user control: pause, abort, resize, and cancellation semantics
  must remain unchanged.
- Heuristics 4 and 5, consistency/error prevention: CLI flags, diagnostics,
  verification, ordering, and incomplete-output cleanup remain compatible.
- Heuristic 9, recovery: output guards and error translation are explicit
  sprint acceptance criteria.

No user-facing redesign or unresolved UX decision is hidden in the refactor.
Mouse may proceed with phase planning.
