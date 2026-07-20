# Rust Boundary Refactoring Sprint

**Sprint type:** Tier 2 maintenance / technical debt
**Product owner:** Cypher
**Architecture owner:** Morpheus
**Source review:** `agents/morpheus.docs/RustCodeQualityReview_Summary_2026-07-18T19-05.md`

## Goal

Reduce measured Rust complexity and coupling without changing compression
semantics, output compatibility, terminal behavior, bounded-memory guarantees,
or materially regressing throughput.

## Scope

### In scope

- Remove the File-to-Stdout whole-input allocation.
- Separate typed TUI state, pure reduction, layout/view models, rendering,
  terminal runtime, and platform probing.
- Separate Stream reader, worker, and ordered-writer responsibilities.
- Separate Split range planning, slice execution, artifact ownership, and
  ordered concatenation.
- Encapsulate controller synchronization and progress-delivery policy.
- Consolidate codec copy/control loops without adding copies in hot paths.
- Separate validated application planning, execution, and output cleanup.

### Out of scope

- Rewriting the compressor or changing archive formats.
- Replacing the standard channel implementation.
- Changing Stream ordering, Split concatenation, worker gating, buffer
  ownership, compression defaults, or live-control semantics.
- Redesigning the TUI or CLI.
- Combining refactoring with unrelated feature work.
- CI enforcement from Backlog 9.4; that remains a follow-on Tank task after
  the baseline is clean.

## User Stories and Acceptance Criteria

### Story 1 — Bounded File-to-Stdout streaming

As a user compressing a large file to standard output, I want memory usage to
remain bounded so input size does not determine process RSS.

Acceptance criteria:

1. File-to-Stdout opens and streams the source instead of reading the complete
   file into memory.
2. File/stdin and file/stdout pairing tests preserve valid, equivalent
   decompressed output.
3. A large-input File-to-Stdout regression demonstrates RSS bounded by
   configured buffers, queues, workers, encoders, and telemetry history.
4. Cancellation and write failures preserve existing cleanup and error
   behavior.

### Story 2 — Typed and testable TUI boundaries

As a maintainer, I want mode-specific state and pure UI transformations so
Stream and Split behavior can evolve without invalid mixed state or a
monolithic event loop.

Acceptance criteria:

1. TUI state contains an explicit `ModeState::{Stream, Split}` and typed
   lifecycle states instead of mode/status string combinations.
2. Progress reduction and layout/view-model calculations are independently
   testable and do not perform terminal I/O.
3. Stream and Split rendering, terminal runtime, and platform probing have
   separate module boundaries.
4. Existing 80x22 and expanded-terminal snapshots and real-PTY completion,
   keyboard, mouse, pause, abort, and telemetry behavior remain equivalent.
5. No production function in the refactored TUI exceeds the configured
   complexity threshold.

### Story 3 — Explicit compression-mode orchestration

As a maintainer, I want Stream and Split roles independently testable so
ordering, cancellation, cleanup, and bounded-memory contracts are visible.

Acceptance criteria:

1. Stream reader, worker, and ordered-writer loops are separate units behind a
   runtime boundary.
2. Split range planning, slice execution, artifact cleanup, and concatenation
   are separate units.
3. Stream output order, dynamic worker gating, bounded queues, buffer reuse,
   Split concatenation order, and temporary-artifact cleanup are unchanged.
4. No new channel dependency or algorithmic redesign is introduced.
5. Split and Stream profiles show no unexplained throughput regression above
   5%, with liblzma remaining the dominant sampled CPU cost.

### Story 4 — Encapsulated controls and shared codec loops

As a maintainer, I want validated control APIs and one telemetry/copy policy so
callers cannot bypass invariants and codec behavior does not drift.

Acceptance criteria:

1. Controller atomics are private and exposed through a snapshot plus
   validated command methods with consistent result contracts.
2. Best-effort progress delivery has one explicit disconnection policy.
3. Gzip, Bzip2, and Xz share the copy/control loop while codec-specific code
   creates and finalizes encoders.
4. Direct slice writes, reusable dynamic buffers, pause, resize, abort, and
   encoder-boundary level semantics remain unchanged.
5. The abstraction adds no mandatory heap boxing or extra hot-loop copies.

### Story 5 — Small application shell and reliable cleanup

As a maintainer, I want execution planning and output ownership explicit so
validation, thread errors, and incomplete-output cleanup are easy to reason
about.

Acceptance criteria:

1. Argument and I/O validation produce a typed `RunPlan`/`ResolvedIo`.
2. An output guard owns incomplete-output cleanup and disarms only after
   successful completion.
3. Pipeline/TUI execution, thread joining, verification, and error translation
   are separated from argument resolution.
4. Signal handlers remain signal-safe.
5. CLI flags, exit behavior, diagnostics, verification, and output cleanup
   remain user-compatible.

## Architecture and Sequencing

The sprint is incremental, not a rewrite:

1. Establish the bounded File-to-Stdout path and characterization coverage.
2. Refactor typed TUI state and pure seams before moving runtime/rendering.
3. Extract Stream roles, then Split roles, without changing concurrency
   algorithms.
4. Encapsulate controller/progress APIs before consolidating codec loops.
5. Simplify the application shell only after compression-mode APIs stabilize.

Binding invariants:

- Preserve direct encoder writes and reusable chunk ownership.
- Preserve bounded queues and ordered Stream/Split output.
- Preserve cancellation, temporary-file, and incomplete-output cleanup.
- Preserve current compression defaults and interactive control semantics.
- Prefer concrete, testable boundaries over a generic framework.

## Validation Gates

After code changes in each phase, run focused Rust tests for that phase, then:

- `make rust-format-check`
- `make rust-clippy`
- `make rust-complexity`
- `make rust-cyclomatic`
- `make rust-dead-code`

Do not rerun unchanged gates between edits. After orchestration and codec/control
phases, rerun the existing Split and Stream performance profiles. Any
unexplained throughput regression above 5%, input-size-dependent memory growth,
output mismatch, ordering failure, or TUI/CLI behavior regression blocks the
phase.

## Sprint Definition of Done

- Strict Clippy and configured complexity gates pass.
- No streaming path allocates the whole input.
- TUI, Stream, Split, controller, codec, and application-shell boundaries are
  independently testable.
- Compression equivalence, ordering, cleanup, bounded memory, live controls,
  snapshots, and real-PTY behavior pass.
- Measured throughput remains within the 5% regression budget.
