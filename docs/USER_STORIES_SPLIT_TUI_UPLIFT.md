# Split-mode TUI Uplift — User Stories

## Sprint Goal

Bring Split mode to the same observability and responsive-UX standard as Stream mode while remaining truthful about fixed startup partitions and controls.

## Story 1 — Aggregate Split Progress

As an operator compressing a regular file, I want an immediate whole-job summary so I can understand completion, throughput, output size, ratio, and remaining work without mentally combining sector rows.

### Acceptance Criteria

- Show aggregate input processed, output produced, total percentage, compression ratio, active/completed sector count, elapsed time, and ETA.
- Aggregate values derive from authoritative per-sector progress and never exceed valid bounds when a compressor reports repeated progress callbacks.
- Completion remains visible until the TUI exits.
- Labels remain readable without color and at the 80x22 minimum.

## Story 2 — Sector Lifecycle Board

As an operator, I want every Split partition to have a stable identity and explicit lifecycle state so I can see which sectors are waiting, running, or complete.

### Acceptance Criteria

- Display operator-facing sectors as one-based `S01`, `S02`, and so on; internal indexes remain zero-based.
- Each visible sector shows textual `WAIT`, `RUN`, or `DONE`, processed/total bytes, percentage, output bytes when known, and per-sector ratio when complete.
- Sector progress bars scale with available width and retain numeric percentage.
- When all sectors cannot fit, show the visible range and explicit overflow count.
- Provide discoverable `PgUp`/`PgDn` sector paging; paging never changes compression state.
- Larger terminals display more sector rows automatically.

## Story 3 — Mirrored Split I/O History

As an operator, I want Split mode to use the same input/output graph vocabulary as Stream mode so that I can compare ingestion/compression work with completed output consistently.

### Acceptance Criteria

- Reuse the mirrored chart: aggregate processed input above the baseline and aggregate compressed output below it.
- `I` toggles RATE and CUMULATIVE without clearing history.
- RATE reports bytes per second from actual sample duration; CUMULATIVE reports totals.
- Both series share one scale, newest samples appear at the right, and final history is retained.
- Empty/waiting intervals visibly produce zero rate rather than stale activity.

## Story 4 — Truthful Split Controls

As an operator, I want controls to distinguish live settings from startup-fixed settings so that I do not believe a click changed partitioning when it did not.

### Acceptance Criteria

- Pause and Throttle remain visibly interactive.
- Compression Level is interactive only if the underlying compressor can apply it to unfinished work; otherwise it is visibly `FIXED`.
- Chunk Size is labeled `PARTITION FIXED` in Split mode and cannot receive focus, keyboard adjustment, or mouse changes.
- Workers is labeled `POOL FIXED` in Split mode and cannot receive focus, keyboard adjustment, or mouse changes.
- Tab order skips non-interactive Split controls.
- Stream mode retains all four live controls unchanged.

## Story 5 — Responsive Split Layout

As an operator using different terminal sizes, I want Split mode to use available space for sector visibility, chart history, logs, and controls without losing the minimum layout.

### Acceptance Criteria

- Preserve 80x22 as the minimum supported size.
- Extra width improves sector details and chart history.
- Extra height is divided among sector rows, chart height, logs, and control gauges.
- Panel borders, titles, status text, and mouse targets remain aligned at 80x22 and 120x30.
- Split and Stream snapshots independently lock their mode-specific behavior.

## Quality and UX Gates

- Deterministic tests cover aggregation, lifecycle state, one-based labels, overflow/paging, Split chart sampling, fixed-control focus/mouse behavior, and responsive geometry.
- Existing byte-perfect Split compression and Stream-mode behavior must not regress.
- Smith must test actual Split-mode compression at 80x22 and 120x30.

## Out of Scope

- Runtime repartitioning of a file.
- Resizing the Rayon pool after Split compression starts.
- Changing the Split file format or sequential output order.
- Persistent telemetry or chart export.
