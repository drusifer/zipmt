# Pipeline Flow Observability and Runtime Controls

## Sprint Goal

Make every chunk's journey through the compression pipeline visible in the TUI, add safe runtime controls for chunk size and active worker count, and use maximum compression by default in every operating mode.

## Story 1: Per-slot pipeline visibility

As an operator, I want each input, worker, pending, and output-sort slot to display its current chunk number so I can understand pipeline flow, stalls, and out-of-order completion at a glance.

### Acceptance Criteria

1. The TUI renders separate, clearly labeled regions for input queue, workers, pending/output-sort, and output-ready stages.
2. Each occupied slot displays a stable one-based chunk number (for example, `#12`); empty slots display a consistent empty marker rather than stale data.
3. A chunk moves through observable states: queued for input, assigned to a worker, completed but pending order, and ready/written in output order.
4. Out-of-order worker completion remains visible in pending/output-sort until all earlier chunk numbers are written.
5. Slot state is driven only by generic pipeline progress events; the pipeline does not reference TUI types.
6. Stream completion, pause, abort, and error paths leave no chunk simultaneously displayed in contradictory stages.

## Story 2: Runtime chunk-size control

As an operator, I want a chunk-size knob so I can tune throughput, memory use, and interactivity while a stream is running.

### Acceptance Criteria

1. The control panel includes a focusable chunk-size knob with keyboard and mouse behavior consistent with existing controls.
2. Supported values are discrete powers of two from 64 KiB through 8 MiB; the default is 1 MiB.
3. A change applies to chunks read after the update. Already queued or running chunks retain their original boundaries and chunk numbers.
4. Changing chunk size never drops, duplicates, corrupts, or reorders output data.
5. The selected size and unit are always visible, and the pipeline controller exposes a thread-safe update method.

## Story 3: Runtime active-worker control

As an operator, I want a worker-count knob so I can change CPU concurrency without restarting compression.

### Acceptance Criteria

1. The control panel includes a focusable workers knob ranging from 1 to the pipeline's configured worker capacity.
2. Increasing the value permits more workers to accept subsequent chunks; decreasing it prevents excess workers from accepting new chunks after their current chunk finishes.
3. Active, idle, and disabled worker slots are visually distinct and retain stable worker identifiers.
4. Worker-count changes never terminate an in-flight chunk and never drop, duplicate, or reorder output.
5. The pipeline controller exposes a thread-safe update method and the TUI shows the active/configured count.

## Story 4: Maximum-compression defaults

As an operator, I want the best compression ratio by default so every CLI, split-file, stream, raw, and TUI path behaves consistently unless I explicitly choose another level.

### Acceptance Criteria

1. The CLI default compression level is 9 for gzip, bzip2, and xz.
2. Pipeline controller state and all TUI constructors initialize to level 9 when the user does not provide a level.
3. Explicit levels 1 through 9 continue to override the default in every mode.
4. Help text, tests, examples, and visible TUI state agree on level 9 as the default.

## Story 5: Regression-proof UX

As a maintainer, I want deterministic state and layout tests so queue visualization and controls remain trustworthy.

### Acceptance Criteria

1. Unit tests cover every chunk-stage transition, including out-of-order completion and ordered release.
2. Tests cover runtime chunk-size and active-worker updates at their minimum, maximum, and intermediate values.
3. Ratatui `TestBackend` tests verify labels, chunk numbers, empty/disabled states, focus cycling, and selected knob values.
4. Existing split, stream, CLI, and TUI behavior remains passing under bounded Makefile test targets.

## UX Guardrails (Smith Gate 1)

1. Stage meaning must not rely on color alone: every region has a text label and each worker shows a textual state such as `RUN`, `IDLE`, or `OFF`.
2. The flow reads left-to-right in processing order: Input Queue → Workers → Pending Sort → Output Ready.
3. Chunk identity uses one notation everywhere (`#N`) and never mixes zero-based internal IDs with one-based display IDs.
4. Focus order follows the visual control order and includes all four knobs: compression, throttle, chunk size, workers, then no focused control.
5. Each focused knob shows both its current value and keyboard affordances; mouse is optional, never the only discoverable input method.
6. Narrow or short terminals degrade by reducing visible slot counts and showing an overflow count (for example, `+3`) rather than clipping labels or silently hiding active chunks.

## Out of Scope

- Changing compression formats or archive compatibility.
- Persisting knob settings between invocations.
- Resizing the maximum worker pool after pipeline startup.
- Showing byte-level subprogress within a single chunk.
