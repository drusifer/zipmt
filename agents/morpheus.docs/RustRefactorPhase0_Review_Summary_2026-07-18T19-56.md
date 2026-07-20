# Rust Refactor Phase 0 Architecture Review

**Verdict:** Approved for Phase 1

Phase 0 removes the only known input-size-dependent streaming allocation and
adds proportional, real-binary characterization of all four I/O pairings,
ordering/equivalence, Linux RSS, and interrupt cleanup. It does not alter
Stream or Split algorithms, ownership, channel topology, or output formats.

The strict-Clippy and cognitive-complexity failures are identical in kind and
location to the analyzer baseline that created this sprint. No Phase 0 code is
named. They are therefore carried as binding acceptance work to their owning
TUI and orchestration phases rather than used to deadlock the prerequisite
correctness phase. They still block final sprint completion.

Task 1.1 may begin. It must introduce typed mode/lifecycle state incrementally
and preserve event semantics; it must not simultaneously move rendering or
runtime code.
