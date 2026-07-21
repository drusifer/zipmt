# Rust Presentation and Dispatch Refactor

**Sprint type:** Tier 2 maintenance / technical debt
**Product owner:** Cypher
**Architecture owner:** Morpheus

## Goal

Reduce the remaining concentration of presentation and dispatch logic while
preserving terminal output, controls, progress semantics, I/O behavior,
compression compatibility, cleanup, and measured performance.

## Evidence

The current deterministic quality baseline is green. Exported metrics identify
three remaining concentrations:

- `draw_tui_impl`: 890 SLOC, cognitive 182, cyclomatic 89.
- `reduce_progress_event`: 129 SLOC, cognitive 39, cyclomatic 34.
- `CompressionPipeline::run`: 93 SLOC, cognitive 12, cyclomatic 14, with all
  four I/O pairings dispatched from one function.

These are maintainability targets, not measured compression bottlenecks.

## Story 1 — Composable TUI rendering

As a maintainer, I want the dashboard assembled from explicit layout, header,
work-panel, chart, log, and control renderers so each visible region can change
without editing an 890-line function.

Acceptance criteria:

1. Split and Stream panels have named renderer boundaries with narrow inputs.
2. Layout calculation is independent of terminal drawing.
3. Existing 80x22 and 120x30 snapshots remain visually identical except for
   separately approved UX changes; this sprint plans none.
4. Page and wheel navigation, completion state, controls, charts, logs, and
   small-terminal fallback remain unchanged.
5. No extracted production renderer exceeds cognitive complexity 20.

## Story 2 — Reducer event families

As a maintainer, I want Split, Stream flow, worker, and terminal lifecycle
events reduced by focused pure functions so state-transition invariants are
obvious and independently testable.

Acceptance criteria:

1. The public reduction entry point remains exhaustive over `ProgressEvent`.
2. Split, stream-flow, worker/chunk, and completion/error transitions have
   focused helpers and tests.
3. Timestamp injection and returned effects remain deterministic.
4. Queue ordering, one-based display identity, worker histories, completion
   cleanup, and error cleanup are unchanged.
5. No reducer production function exceeds cognitive complexity 20.

## Story 3 — I/O execution adapters

As a maintainer, I want pipeline input/output pairing resolved through explicit
execution adapters so resource ownership and error translation are not mixed
into thread orchestration.

Acceptance criteria:

1. File/File, File/Stdout, Stdin/File, and Stdin/Stdout use named adapters.
2. File/File remains Split; all other pairings remain Stream.
3. Bounded memory, output ownership, progress completion/error policy,
   cancellation, and cleanup semantics remain unchanged.
4. Existing four-pairing decompression and SIGINT tests pass.
5. The compression codec and Stream/Split hot loops are not redesigned.

## Architecture

### Phase order

1. Extract render context/layout and mode-specific panels behind the existing
   `draw_tui` entry point.
2. Split the pure reducer by event family without changing its caller.
3. Introduce pipeline execution adapters behind `CompressionPipeline::run`.

### Boundaries

- `tui/render.rs` owns frame composition and delegates to panel modules.
- Renderer helpers receive immutable state/view data and Ratatui areas; they do
  not read events, atomics, process state, or clocks.
- `tui/reducer.rs` retains exhaustive dispatch; focused helpers own mutations
  for one coherent event family.
- Pipeline adapters own concrete stdin/stdout/file handles and call the
  existing Split or Stream functions. The orchestration thread owns reporting
  `Complete` or `Error`.

### Constraints

- No UX redesign, flag change, archive-format change, or channel replacement.
- No new allocations or dynamic dispatch in compression hot loops.
- Existing benchmark workload hash remains the longitudinal performance anchor.

## Quality and performance gates

- Focused tests precede full validation after changed code.
- `make rust-format-check`
- `make rust-clippy`
- `make rust-complexity`
- `make rust-cyclomatic`
- `make rust-dead-code`
- Full Rust tests and release build at phase and sprint gates.
- `make rust-benchmark RUNS=3` at sprint close; no unexplained regression above
  5% against the last same-workload record.

## Definition of Done

- The three identified concentrations have explicit, independently testable
  boundaries.
- TUI snapshots and real user interaction contracts are preserved.
- Reducer semantics and all four I/O pairings pass.
- Deterministic quality gates remain green.
- The revision-tagged benchmark remains within the 5% budget.
