# Task Board: Rust Non-Functional Boundary Enhancements

**Status:** Complete
**Work type:** Non-functional enhancement
**Sprint tier:** Tier 2 maintenance / technical debt
**Epic:** `docs/EPIC_RUST_REFACTOR_3.md`
**Architecture:** `docs/ARCH_RUST_REFACTOR_3.md`

## Objective

Improve maintainability, modularity, testability, and terminal failure-path
reliability without adding features or changing observable behavior.

## Phase 0: Dashboard Panel Boundaries

- [x] **Task 0.1 — Characterize panel contracts:** Lock Split, Stream,
  I/O/process, logs, footer/controls, small-terminal, and completion rendering
  with focused assertions around the existing snapshots.
  (Assignee: Neo | UAT: Trin)
- [x] **Task 0.2 — Extract standalone panels:** Move each panel body from
  `draw_tui_impl` into a named renderer with narrow immutable context.
  (Assignee: Neo | UAT: Trin | UX: Smith | Review: Morpheus)

**Gate:** `draw_tui_impl` is a bounded coordinator; snapshots are unchanged;
each extracted renderer is at or below cognitive complexity 20.

## Phase 1: Chart Composition

- [x] **Task 1.1 — Extract chart view model:** Separate scale, guides, labels,
  averages, and series preparation from Ratatui widget construction.
  (Assignee: Neo | UAT: Trin)
- [x] **Task 1.2 — Extract chart widget renderer:** Build axes/datasets from the
  view model and preserve rate/cumulative and mirrored presentation.
  (Assignee: Neo | UAT: Trin | UX: Smith)

**Gate:** `render_history_chart` is replaced by bounded preparation/rendering
functions; chart and dashboard snapshots remain unchanged.

## Phase 2: Terminal Runtime Lifecycle

- [x] **Task 2.1 — Terminal session guard:** Add RAII ownership for raw mode,
  alternate screen, mouse capture, and restoration.
  (Assignee: Neo | UAT: Trin)
- [x] **Task 2.2 — Runtime role extraction:** Separate event polling, progress
  draining, frame ticks, and pipeline join/error handling.
  (Assignee: Neo | UAT: Trin | Review: Morpheus)

**Gate:** terminal restoration passes success, abort, compression-error, and
covered panic paths; runtime coordinator stays below complexity thresholds.

## Phase 3: Command-Family Dispatch

- [x] **Task 3.1 — Keyboard command families:** Extract global, work-list,
  control, and log navigation dispatchers with unchanged precedence.
  (Assignee: Neo | UAT: Trin)
- [x] **Task 3.2 — Keyboard/mouse consistency:** Reuse state commands where
  practical and verify mode-specific navigation/control behavior.
  (Assignee: Neo | UAT: Trin | UX: Smith)

**Gate:** input behavior and mouse regions are unchanged; production
dispatchers remain below configured thresholds.

## Phase 4: Application Startup Services

- [x] **Task 4.1 — Startup policy services:** Extract typed compressor
  resolution, signal installation, cleanup registration, and pure exit-code
  mapping from `main`.
  (Assignee: Neo | UAT: Trin)
- [x] **Task 4.2 — Epic validation:** Run full behavioral, UX, quality,
  release, audit, cleanup, and longitudinal performance gates.
  (Assignee: Trin | UX: Smith | Review: Morpheus)

**Gate:** CLI help, diagnostics, verification, cleanup, and exit codes are
unchanged; all deterministic gates pass; performance is within 5%.

## Non-Functional Requirements

- Maintainability: reduce concentration and coupling.
- Testability: each boundary is independently exercised.
- Reliability: terminal and output cleanup ownership is explicit.
- Compatibility: no observable CLI/TUI behavior changes.
- Performance: no unexplained same-workload regression above 5%.

## Out of Scope

- Features, UX redesign, new controls, or changed CLI/error text.
- Compression codecs, archive formats, channels, or worker algorithms.
- CI and deployment changes.

## Definition of Done

- [x] All five non-functional enhancement stories pass.
- [x] Target coordinators and dispatchers are bounded.
- [x] Snapshots and binary behavior remain unchanged.
- [x] Full deterministic quality and release gates pass.
- [x] Revision-tagged benchmark remains within budget.
