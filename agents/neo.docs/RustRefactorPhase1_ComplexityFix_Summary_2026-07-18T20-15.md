# Rust Refactor Phase 1 Complexity Fix

The first fix iteration split terminal event policy into bounded key, mouse,
resize, scrolling, paging, and focused-control helpers in `tui/runtime.rs`.
Rendering now isolates Split, Stream, I/O chart, logs, and footer work into
bounded closures behind the render module entry.

Results:

- TUI runtime and rendering no longer appear in the cognitive-complexity gate.
- Strict Clippy passes with zero findings.
- Full Rust suite passes.
- Real-PTY compression, integrity, dashboard rendering, and terminal cleanup
  pass at 80x22 and 120x30 through `make rust-pty-smoke`.
- The only remaining complexity findings are the three planned Phase 2
  Stream/Split orchestration hotspots.
