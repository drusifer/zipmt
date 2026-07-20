# Rust Code Quality Review

**Reviewer:** Morpheus
**Date:** 2026-07-18
**Scope:** `zipmt-rust` architecture, quality, complexity, coupling, duplication,
ownership boundaries, memory behavior, and performance-sensitive refactoring.

## Executive Assessment

The Rust implementation does not need a rewrite. Its compression behavior,
bounded Split design, ordering model, and direct encoder output path are worth
preserving. It does need structural refactoring: UI state, event handling,
terminal runtime, rendering, and platform probing have accumulated in one
3,119-line module, while both compression modes place several concurrency
responsibilities inside single functions and closures.

The highest-value change is to restore boundaries without changing algorithms.
First fix the remaining unbounded File-to-Stdout read, then separate the TUI
state/reducer/runtime/rendering layers, then extract Stream and Split
orchestration components. Compressor-loop deduplication and CLI cleanup should
follow once those behavioral seams are protected by tests.

## Analyzer Evidence

| Check | Result | Architectural signal |
|---|---:|---|
| `make rust-format-check` | Pass | Formatting is consistent. |
| `make rust-clippy` | Fail, 23 errors | Mostly TUI control-flow duplication and nesting; also error construction, checked division, iteration, and repeated branches. |
| `make rust-complexity` | Fail | Five production hotspots exceed the configured cognitive threshold. |
| `make rust-cyclomatic` | Pass/artifact produced | The configured command succeeds, but the metrics identify severe local hotspots. |
| `make rust-dead-code` | Pass | No current dead-code accumulation. |
| Existing audit/safety baseline | Pass | Main crate denies unsafe code; no dependency advisory was recorded in the current baseline. |
| Existing Split/Stream profiles | Encoder-dominated | Roughly 87% of sampled CPU is in liblzma; orchestration is not a material CPU hotspot. |

The detailed `rust-code-analysis` artifact reports:

- `tui::run_tui_on_main_thread`: cyclomatic 83, cognitive 149.
- `tui::draw_tui`/render closure: cyclomatic 67, cognitive 158.
- `tui::apply_progress_event`: cyclomatic 33, cognitive 47.
- `main::run_app`: cyclomatic 30, cognitive 39.
- Stream worker closure: cyclomatic 17, cognitive 48.
- Stream reader closure: cyclomatic 13, cognitive 34.
- Split `compress_file`: cyclomatic 17.

These figures are more actionable than the aggregate pass/fail status: they
show where unrelated responsibilities have collapsed together.

## Principal Smells and Recommended Boundaries

### 1. TUI god module and invalid state combinations

`tui.rs` owns state models, Split and Stream projections, event reduction,
Linux process telemetry, terminal probing, keyboard/mouse behavior, the main
event loop, every widget, layout calculations, formatting, and tests.
`TuiState` is a flat union of mutually exclusive Split and Stream fields, so
invalid mode-specific combinations are representable. Its constructors repeat
large initialization blocks.

Refactor toward:

```text
tui/
  mod.rs
  state.rs             TuiState + ModeState::{Stream, Split}
  reducer.rs           pure ProgressEvent -> state transitions
  runtime.rs           polling, control commands, completion lifecycle
  platform.rs          terminal size and process telemetry
  render/
    layout.rs
    stream.rs
    split.rs
    io_chart.rs
    controls.rs
```

Make layout calculation and reducer behavior pure. Rendering should consume
view models, not mutate operational state. Replace worker status strings with a
typed lifecycle enum. Consolidate overlapping worker/chunk events so each
transition has one authority.

### 2. Stream orchestration function owns four jobs

`compress_stream` creates shared state and channels, then embeds reader,
worker, coordinator, and ordered-writer behavior. The worker and reader
closures are analyzer hotspots. `Arc<Mutex<Receiver<Block>>>` is an
implementation consequence of the standard MPSC receiver and should be hidden
behind the worker-pool boundary rather than leaked through orchestration.

Extract:

- `StreamRuntime`: controller, progress sink, counters, and channels.
- `reader_loop`: fill/reuse chunk buffers and enqueue numbered blocks.
- `worker_loop`: eligibility, compression, and result publication.
- `ordered_writer`: pending-map ordering and output accounting.

Do not introduce a new channel dependency in the first pass. Preserve the
current bounded queue, direct chunk ownership transfer, ordering, and dynamic
worker semantics. Reconsider MPMC only after the extracted version is measured.

### 3. Split orchestration mixes planning, execution, and assembly

`split_mode::compress_file` plans ranges, builds the pool, performs each
seeked-range compression, manages temporary files, emits progress, and
concatenates results.

Extract `SliceRange`, `compress_slice`, `SliceArtifact`, and
`concatenate_slices`. Keep one reusable read chunk per worker and pass the temp
file directly as the encoder writer. Make the artifact responsible for cleanup
unless it is successfully consumed.

### 4. One remaining unbounded-memory I/O path

The File-to-Stdout branch in `CompressionPipeline::run` reads the entire input
with `std::fs::read` before creating a cursor. This violates the fixed-memory
model established for the other paths.

Open the file and pass the reader directly to `compress_stream`. Add a
large-input RSS regression test for this exact I/O pairing.

### 5. Weak controller and event encapsulation

`PipelineController` exposes atomic fields publicly, allowing callers to
perform unchecked loads/stores and know its synchronization representation.
Its update methods also have inconsistent result contracts.

Make atomics private. Expose a `ControllerSnapshot`, validated command methods,
and consistent results. Wrap best-effort progress sends in a `ProgressSink` so
observer disconnection policy is explicit rather than repeated as ignored
`send` results.

### 6. Repeated compressor loops

Gzip, Bzip2, and Xz repeat reader-to-writer and slice-to-writer control loops.
The codecs should create/finalize encoders; one shared copy/control loop should
own resizing, pause/abort checks, buffer reuse, and progress.

Preserve the direct slice write path and the single reusable dynamic buffer.
Avoid a generic abstraction that forces extra copies or boxes the hot loop.

### 7. CLI execution and cleanup boundaries

`run_app` combines argument validation, I/O resolution, verification, output
registration, pipeline/TUI construction, thread joining, and error mapping.
Global incomplete-output cleanup is duplicated.

Create a validated `RunPlan`/`ResolvedIo`, a small execution layer, and an
`OutputGuard` that disarms only on success. Keep signal-handler constraints
explicit; do not move non-signal-safe work into the handler.

## Prioritized Refactoring Sequence

### Phase 0 — Correctness and characterization

1. Replace the File-to-Stdout whole-file read with streaming.
2. Add focused tests for all four file/stdin and file/stdout pairings,
   compressed-output verification, cancellation cleanup, and output order.
3. Add bounded-RSS coverage for File-to-Stdout.

**Risk:** Low implementation risk, high correctness value.

### Phase 1 — Typed state and pure TUI seams

1. Introduce `ModeState::{Stream, Split}` and typed worker/slice lifecycle.
2. Extract the event reducer and pure layout/view-model calculations.
3. Split rendering by panel, then isolate terminal runtime/platform probing.
4. Preserve current snapshots and real-PTY behavior throughout.

**Risk:** Medium. The danger is subtle UX/state regression, not compression.
Move one seam at a time and keep snapshots green after each move.

### Phase 2 — Compression-mode orchestration

1. Extract Stream reader, worker, and ordered-writer loops.
2. Extract Split range planning, slice execution, artifacts, and concatenation.
3. Centralize runtime/progress plumbing without altering buffer ownership.

**Risk:** High. Ordering, cancellation, dynamic worker gating, bounded queues,
and temp cleanup are concurrency contracts. Do not combine this phase with
channel-library changes.

### Phase 3 — Compressor and controller APIs

1. Make controller synchronization private and introduce snapshot/command APIs.
2. Centralize best-effort telemetry policy.
3. Deduplicate codec copy/control loops while retaining direct writes.
4. Replace `Arc<Box<dyn Compressor>>` with `Arc<dyn Compressor>`.

**Risk:** Medium. Validate pause, resize, abort, and level-change semantics at
chunk/encoder boundaries.

### Phase 4 — Application shell and cleanup

1. Extract argument-to-`RunPlan` validation and I/O resolution.
2. Introduce recoverable incomplete-output ownership.
3. Simplify thread join and error translation.

**Risk:** Medium-low after mode APIs are stable.

## Validation Gates

Every phase must pass:

- `make rust-format-check`
- `make rust-clippy` with zero warnings
- `make rust-complexity` with no production function above the configured
  threshold
- `make rust-cyclomatic`
- `make rust-dead-code`

Run focused Rust tests only after the associated code changes. Required
behavioral gates are decompression equivalence, Stream ordering, Split
concatenation order, bounded queues/RSS, abort cleanup, live resize controls,
80x22 and expanded-terminal TUI snapshots, and real-PTY completion behavior.

After Phases 2 and 3, rerun Split and Stream profiles. Accept no unexplained
throughput regression above 5%; liblzma should remain the dominant CPU cost.
Memory growth must be bounded by configured chunks, workers, queues, encoder
state, and chart history—not input size.

## Definition of Done

- Zero strict-Clippy findings and no configured complexity violations.
- No whole-input allocation in any streaming path.
- TUI reducer, runtime, platform integration, and rendering are separate.
- Stream and Split orchestration roles are independently testable.
- Controller atomics are private and state transitions are typed.
- Compression output, ordering, memory bounds, and measured throughput remain
  equivalent within the stated gates.
