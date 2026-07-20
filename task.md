# Task Board: Rust Boundary Refactoring Sprint

**Status:** Sprint complete
**Sprint type:** Tier 2 maintenance / technical debt
**Specification:** `docs/USER_STORIES_RUST_REFACTOR.md`
**Architecture evidence:** `agents/morpheus.docs/RustCodeQualityReview_Summary_2026-07-18T19-05.md`

## Sprint Goal

Reduce measured Rust complexity and coupling while preserving compression
compatibility, bounded memory, ordering, cleanup, CLI/TUI behavior, live
controls, and measured throughput.

## Phase 0: Bounded I/O and Characterization

- [x] **Task 0.1 — Stream File-to-Stdout:** Replace the whole-file allocation
  with direct streaming and preserve existing errors and cancellation behavior.
  (Assignee: Neo | UAT: Trin)
- [x] **Task 0.2 — I/O and memory characterization:** Add focused coverage for
  all file/stdin and file/stdout pairings, decompression equivalence, output
  order, cancellation cleanup, and large-input bounded RSS. (Assignee: Neo |
  UAT: Trin | Review: Morpheus)

**Phase gate:** Passed. Focused tests and changed-code checks pass;
File-to-Stdout RSS is bounded below 96 MiB for a 128 MiB input. The unchanged
strict-Clippy and complexity baseline remains binding work for Phases 1 and 2
and blocks sprint completion, not Phase 0 progression.

## Phase 1: Typed and Pure TUI Seams

- [x] **Task 1.1 — Typed mode/lifecycle state:** Introduce
  `ModeState::{Stream, Split}` and typed worker/slice lifecycle while preserving
  event semantics. (Assignee: Neo | UAT: Trin)
- [x] **Task 1.2 — Pure reducer and view models:** Extract independently
  testable progress reduction and layout/view-model calculations with no
  terminal I/O. (Assignee: Neo | UAT: Trin)
- [x] **Task 1.3 — Runtime/render/platform boundaries:** Separate Stream/Split
  rendering, terminal runtime, and platform probing while preserving 80x22 and
  expanded snapshots plus real-PTY controls/completion. (Assignee: Neo |
  UAT: Trin | UX: Smith | Review: Morpheus)

**Phase gate:** Snapshot, reducer, layout, control, telemetry, and real-PTY
checks pass; no refactored TUI production function exceeds the configured
complexity threshold.

## Phase 2: Compression-Mode Orchestration

- [x] **Task 2.1 — Stream roles:** Extract reader, worker, and ordered-writer
  loops behind a runtime boundary without changing channels, buffer ownership,
  dynamic worker gating, cancellation, or ordering. (Assignee: Neo | UAT: Trin)
- [x] **Task 2.2 — Split roles:** Extract range planning, slice execution,
  artifact cleanup, and ordered concatenation while retaining reusable buffers
  and direct encoder writes. (Assignee: Neo | UAT: Trin | Review: Morpheus)

**Phase gate:** Stream ordering, Split concatenation, bounded queues/memory,
abort and temporary-artifact cleanup pass. Split and Stream profiles show no
unexplained throughput regression above 5%.

## Phase 3: Controller, Progress, and Codec Boundaries

- [x] **Task 3.1 — Encapsulated control/progress API:** Make controller atomics
  private; add validated commands, snapshots, consistent results, and one
  explicit best-effort progress policy. (Assignee: Neo | UAT: Trin)
- [x] **Task 3.2 — Shared codec copy/control loop:** Consolidate Gzip, Bzip2,
  and Xz copy/control behavior without extra hot-loop copies or mandatory
  boxing; preserve direct writes and encoder boundaries. (Assignee: Neo |
  UAT: Trin | Review: Morpheus)

**Phase gate:** Pause, resize, abort, level-boundary, decompression, and
progress-disconnection checks pass. Profiles remain within the 5% regression
budget.

## Phase 4: Application Shell and Final Quality Baseline

- [x] **Task 4.1 — Typed run plan and output guard:** Separate argument/I/O
  resolution, execution, joining, verification, error translation, and
  incomplete-output ownership while keeping signal handlers signal-safe.
  (Assignee: Neo | UAT: Trin | UX: Smith)
- [x] **Task 4.2 — Sprint-wide validation:** Verify CLI/TUI compatibility,
  decompression equivalence, ordering, bounded memory, cleanup, live controls,
  snapshots, real PTY behavior, and the complete deterministic Rust quality
  baseline. (Assignee: Trin | UX: Smith | Review: Morpheus)

**Phase gate:** Strict Clippy, complexity, cyclomatic, formatting, and dead-code
targets pass; all acceptance criteria are demonstrated and no unexplained
throughput regression exceeds 5%.

## Required Quality Gates

After changes in each phase, run focused tests for that phase, then:

- `make rust-format-check`
- `make rust-clippy`
- `make rust-complexity`
- `make rust-cyclomatic`
- `make rust-dead-code`

Do not rerun unchanged tests or gates. Performance profiles are required after
Phases 2 and 3 and at final validation only if later changes touch hot paths.

## Out of Scope

- Compression rewrite, archive-format changes, channel replacement, TUI/CLI
  redesign, unrelated features, and algorithm changes.
- Backlog 9.4 CI enforcement. Tank receives that work only after this sprint
  establishes a clean deterministic baseline.

## Definition of Done

- [x] No whole-input allocation remains in any streaming path.
- [x] Strict Clippy and configured complexity gates pass.
- [x] TUI, Stream, Split, controller, codec, and application-shell boundaries
  are independently testable.
- [x] Output compatibility, ordering, cleanup, bounded memory, controls,
  snapshots, and real-PTY behavior pass.
- [x] Throughput remains within the 5% regression budget.

## Final Evidence

- Full Rust suite: 53 library, 5 binary, 11 integration, and doc tests pass.
- Release build and Rust security audit pass.
- Formatting, strict Clippy, cognitive complexity, cyclomatic metrics, and
  dead-code gates pass.
- 128 MiB File-to-Stdout RSS remains below 96 MiB; SIGINT output cleanup passes.
- Split uses unnamed temporary files so process termination cannot strand slice
  artifacts.
- User-tested 80x22 Split/Stream dashboards now provide PgUp/PgDn and scoped
  mouse-wheel work-list scrolling with visible range cues.
- Three alternating 32 MiB XZ level-1 runs: pre-sprint `HEAD` mean 8.281 s;
  refactor mean 8.185 s; delta **-1.15%** (improvement).
- `make rust-benchmark` appends revision-tagged results for the current binary
  to a retained history ledger.
