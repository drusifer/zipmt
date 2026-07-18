# Agent Local Context (context.md)

This file tracks the current state of test findings, patterns, and verification metrics for the QA Guardian (Trin).

## Recent Decisions
- **Rust quality gates established (2026-07-18)**: Make now owns formatter, strict Clippy, cognitive and cyclomatic complexity, dead-code, RustSec/cargo-deny, cargo-geiger, Miri, Valgrind, flamegraph, and bloat workflows. Initial baseline: format/dead-code and audit pass; 22 strict Clippy findings; three paths exceed cognitive threshold 20. Project license is MIT.
- **Graph/worker UAT passed (2026-07-17)**: Verified one-second bucket gating, ten-bucket labels, worker progress reduction/rendering, one-based identity, overflow, 39 unit tests, and 7 integration tests.
- **Slice observability UAT passed (2026-07-17)**: Verified stable ETA, frozen slice averages, CPU/RSS parsers/panels, responsive widget capacity, 37 unit tests, and 7 integration tests.
- **Bounded Split streaming UAT passed (2026-07-17)**: Verified seeked range streaming, live temp-output events, final concatenation totals, byte-perfect output, MA5 color separation, 35 unit tests, and 7 integration tests.
- **TUI completion/smoothed I/O UAT passed (2026-07-17)**: Verified final-state freeze/summary, MA5 projection and labels, exact cumulative mode, automation escape, snapshots, and 34 unit + 7 integration tests.
- **Split TUI uplift UAT passed (2026-07-17)**: Verified typed lifecycle, bounded aggregates, completed-only ratio, Split I/O sampling, one-based responsive board, paging/chart/fixed-control presentation, focus behavior, Stream regressions, and byte-perfect integration. Serial gate passed 31 unit + 7 integration.
- **Mirrored I/O chart Phase 1 UAT passed (2026-07-17)**: Verified rate/cumulative sample retention, bytes-per-second normalization, zero-delta behavior, mirrored input/output orientation, shared baseline labels, responsive mouse boundaries, 80x22 snapshots, exact-state integration 7/7, and real 120x30 evidence.
- **Pipeline Flow Phase 3 UAT passed (2026-07-16)**: After one split-snapshot/overflow-baseline correction, verified labeled flow, explicit overflow, four-control focus/mouse behavior, both mode snapshots, layout family 5/5, and integration 7/7.
- **Pipeline Flow Phase 2 UAT passed (2026-07-16)**: Verified valid/invalid control bounds, future-only chunk resizing, fixed-pool worker disable events, consecutive ordered writes, exact decompression, existing stream behavior, and 7/7 integration tests.
- **Phase 1 UAT rejected once (2026-07-16)**: Pipeline lifecycle/default tests pass, but the existing stream panel exposes zero-based sequence IDs. Operator-facing queue, worker, pending, next, and gap labels must add one while preserving zero-based internal ordering.
- **Phase 1 UAT passed after correction (2026-07-16)**: Verified one-based rendering evidence, 5/5 binary tests, 7/7 integration tests, 2/2 reducer tests, and stream lifecycle/integrity coverage. Phase tasks 1.1–1.3 are complete.
- **Passed Decoupling & Interactive TUI Upgrade UAT Gate**: Fully verified Phase 1 (modular pipeline abstraction, thread-safe PipelineController, restored CLI `-T` / `--tui` flag), Phase 2 (interactive vertical sliders for level and throttle delay, Tab keyboard focus navigation, Up/Down arrow adjustments, and Crossterm mouse click/drag event handling), and Phase 3 (decoupled TUI layout snapshot verification on Ratatui TestBackend).
- **Passed CLI TUI Defaulting & Fallback UAT Gate (Task 1.2)**: Verified that `-T`/`--tui` flag is removed, TUI runs by default, fallback checks correctly bypass TUI when output is redirected or streams are not TTYs, and all 20 unit/integration tests pass cleanly under `make test-rust`.
- **Approved Test Suite**: Verified that all 20 tests (13 unit, 7 integration) pass successfully under `make test-rust` with BypassSandbox: true.
- **Passed Phase 3 Widgets & Layout Snapshots UAT Gate**: Verified that layout snapshot tests compile and assert correctly using Ratatui's `TestBackend` buffer cell symbol assertions. Checked that the LCARS dashboard panels and widgets render correctly in both Split and Stream modes under testing, validating alignment, progress sectors, transporter buffer capacity, and rolling speed history.
- **Passed Phase 2 Event Loop & Throttling UAT Gate**: Verified that the main thread keyboard event polling loop (using crossterm `event::poll`/`read` at 100ms tick rate) operates correctly, manages atomic pause (`IS_PAUSED`) and throttle delay (`THROTTLE_DELAY_MS`), and handles key commands (`+`/`=`, `-`, `p`/`P`, `q`/`Esc` abort paths) properly. Checked that background compression threads cleanly pause and throttle via `check_throttle`.
- **Passed Phase 1 Ratatui Migration UAT Gate**: Verified TUI runs by default on normal compression, and auto-fallback disables TUI correctly when outputs/inputs are not interactive TTYs or redirected to stdout.

---
*Last updated: 2026-07-18T12:27:00-04:00*
