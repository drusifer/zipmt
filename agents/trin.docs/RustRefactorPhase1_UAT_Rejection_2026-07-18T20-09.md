# Rust Refactor Phase 1 UAT — Rejected

**Verdict:** Reject

Typed lifecycle, pure reducer/layout seams, platform/runtime/render entry
boundaries, full tests, formatting, and strict Clippy all pass. However,
`make rust-complexity` still reports:

- `run_tui_on_main_thread_impl`: 39/20 cognitive complexity.
- the `draw_tui_impl` frame closure: 49/20 cognitive complexity.

These are explicit Phase 1 acceptance criteria, not deferrable baseline debt.
Neo must extract bounded event/key/mouse handlers and bounded rendering panel
functions, then rerun changed-code tests and the complexity gate.
