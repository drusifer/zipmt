# Epic: Rust TUI Decomposition and Application Boundaries

**Epic owner:** Cypher
**Architecture owner:** Morpheus
**Work type:** Non-functional enhancement
**Sprint tier:** Tier 2 maintenance / technical debt

> This epic delivers no new user-facing capability. Its outcomes are
> maintainability, modularity, testability, and failure-path reliability.

## Problem

The compression pipeline, reducers, and I/O adapters are now well bounded, but
presentation and application-control logic remains concentrated. The exported
metrics identify `draw_tui_impl` as the dominant maintainability hotspot, with
secondary concentration in chart rendering, terminal runtime, keyboard
dispatch, and application startup.

## Goal

Complete the presentation and application-boundary refactor while preserving
every observable CLI/TUI behavior and avoiding compression hot-path changes.

## Non-functional requirements

- **Maintainability:** reduce concentration and coupling.
- **Testability:** expose independently testable presentation and lifecycle
  boundaries.
- **Reliability:** make terminal restoration and cleanup ownership explicit.
- **Performance:** remain within the established 5% regression budget.
- **Compatibility:** preserve all observable CLI/TUI behavior byte-for-byte or
  snapshot-for-snapshot where applicable.

## Story 1 — Standalone dashboard panels

As a maintainer, I want Split, Stream, I/O/process, logs, and footer/control
panels rendered by standalone functions so a panel can evolve without editing
the entire dashboard.

Acceptance criteria:

1. `draw_tui_impl` becomes a small frame coordinator.
2. Split, Stream, I/O/process, logs, and footer/control panels have named
   standalone renderers with narrow immutable inputs.
3. No panel renderer exceeds cognitive complexity 20.
4. Existing 80x22 and 120x30 snapshots remain identical.
5. Small-terminal, completion, paging, mouse-wheel, focus, logs, and controls
   remain unchanged.

## Story 2 — Chart composition

As a maintainer, I want chart data, axes, datasets, and widget rendering
separated so chart behavior is testable without one branching renderer.

Acceptance criteria:

1. `render_history_chart` is split into preparation and rendering stages.
2. Rate/cumulative modes, mirrored orientation, moving average, guides,
   scaling, and labels remain unchanged.
3. No chart production function exceeds cognitive complexity 20.
4. Deterministic chart and snapshot tests pass.

## Story 3 — Terminal runtime lifecycle

As a maintainer, I want terminal setup/restore, event polling, progress
draining, and pipeline shutdown separated so cleanup behavior is explicit.

Acceptance criteria:

1. Terminal lifecycle is owned by an RAII guard.
2. Event polling, progress draining, frame rendering, and join/error handling
   use focused functions.
3. Raw mode and alternate screen are restored on success, abort, compression
   error, and panic paths covered by tests.
4. `run_tui_on_main_thread_impl` becomes a bounded coordinator.

## Story 4 — Command-family dispatch

As a maintainer, I want keyboard input grouped by command family so adding one
control does not expand a single high-branch match.

Acceptance criteria:

1. Exit/pause/chart commands, work-list navigation, control adjustment, and log
   navigation have focused dispatchers.
2. Key precedence and mode-specific behavior remain unchanged.
3. Mouse dispatch remains consistent with keyboard behavior.
4. Each production dispatcher stays below configured complexity thresholds.

## Story 5 — Application startup services

As a maintainer, I want algorithm selection, signal installation, exit-code
translation, and failure cleanup separated from `main` so startup policy is
independently testable.

Acceptance criteria:

1. Algorithm resolution returns a typed result rather than exiting internally.
2. Signal-handler installation and cleanup registration have an explicit
   boundary.
3. Exit-code mapping is a pure function.
4. Existing CLI help, error text, verification, output cleanup, and exit codes
   remain unchanged.

## Epic-wide constraints

- No compression codec, Stream/Split worker, channel, archive-format, or
  bounded-memory redesign.
- No UX redesign, new controls, renamed flags, or changed error messages.
- Existing snapshots and real-user 80x22 contracts are binding.
- Same-workload performance regression must remain within 5%.

## Definition of Done

- All five stories and their tests pass.
- `draw_tui_impl`, `render_history_chart`, `run_tui_on_main_thread_impl`,
  keyboard dispatch, and `main` are bounded coordinators.
- Full tests, release build, audit, formatter, strict Clippy, complexity,
  cyclomatic, and dead-code gates pass.
- `make rust-benchmark RUNS=3` appends an in-budget record.
