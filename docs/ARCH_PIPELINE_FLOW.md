# Architecture: Pipeline Flow Observability and Runtime Controls

## Decision Summary

Extend the existing event-driven pipeline rather than exposing channel internals to the TUI. Sequence numbers remain the authoritative chunk identity. The stream reader, workers, and ordered writer publish explicit lifecycle events; `TuiState` reduces those events into a visual projection. Runtime chunk size and active-worker limits live in `PipelineController` atomics and affect only future reads/assignments.

## Data Flow

```text
Reader ── ChunkQueued(#N) ──> bounded input queue
                                   │
                                   v
Worker W ─ ChunkAssigned(W,#N) ─ compress ─ ChunkPending(#N)
                                                  │
                                  ordered BTreeMap│
                                                  v
                                      ChunkWritten(#N)

All lifecycle events ──> ProgressEvent channel ──> TuiState reducer ──> LCARS flow panel
```

## Progress Event Contract

Add typed variants to `ProgressEvent`:

```rust
ChunkQueued { seq_num: u64, bytes: usize },
ChunkAssigned { worker_id: usize, seq_num: u64 },
ChunkPending { worker_id: usize, seq_num: u64 },
ChunkWritten { seq_num: u64 },
WorkerAvailability { worker_id: usize, enabled: bool },
```

Events describe domain state, not presentation. Display numbers are `seq_num + 1`; internal ordering remains zero-based. Event order for one chunk is monotonic. Error/abort/complete clears transient visual state in the reducer.

## Controller Contract

Extend `PipelineController` with:

```rust
chunk_size: Arc<AtomicUsize>,
active_workers: Arc<AtomicUsize>,
max_workers: usize,

update_chunk_size(bytes: usize), // powers of two, 64 KiB..8 MiB
update_active_workers(count: usize), // clamped to 1..=max_workers
```

Defaults are 1 MiB chunks, all configured workers active, throttle 0, and compression level 9. Controller construction receives maximum worker capacity so validation is centralized.

## Runtime Semantics

- The reader loads `chunk_size` immediately before allocating/reading the next block. Existing blocks keep their boundaries.
- Workers have stable IDs. A worker whose ID is outside `0..active_workers` reports `OFF` and does not accept new work.
- Worker receive uses bounded polling so changes are observed promptly. If a worker becomes disabled between eligibility check and receive, it returns the untouched sequence-tagged block to the queue before doing work.
- Decreasing active workers never interrupts a running encoder. Increasing makes eligible workers resume queue consumption.
- The writer retains the existing `BTreeMap<u64, Vec<u8>>` ordering authority. `ChunkPending` fires after compression result insertion; `ChunkWritten` fires only after successful `write_all`.

## TUI Projection

`TuiState` gains `chunk_size`, `active_workers`, and explicit stage collections. A single reducer method applies every `ProgressEvent`, eliminating duplicated match logic in normal and final-drain paths.

The stream flow panel renders:

```text
INPUT QUEUE       WORKERS                 PENDING SORT       OUTPUT READY
#14 #15 +2        W0 RUN #12  W1 IDLE     #13 #16            next #13
                  W2 OFF     W3 RUN #11
```

Visible slots are derived from available width. Hidden occupied slots produce a `+N` suffix. Worker state is textual (`RUN`, `IDLE`, `HOLD`, `OFF`) and color is supplementary.

The footer becomes four equal control cards: Level, Throttle, Chunk, Workers. `Tab` cycles visual order. Up/Down and mouse interaction use common value-to-position helpers. Chunk values use a constant table; workers use `1..=max_workers`.

Split mode keeps its stripe visualization. The same four controls remain visible, but chunk-size and worker changes apply only before future stream chunks; where a completed split partition cannot be resized, the UI labels the control `NEXT` rather than implying that an in-flight stripe changed.

## Maximum-compression Default

Introduce `DEFAULT_COMPRESSION_LEVEL: u32 = 9` in the library and use it for CLI parsing, controller/test helpers, TUI fixtures, and compressor convenience calls. Explicit CLI values remain authoritative.

## Testing Strategy

1. Reducer unit tests for queued → assigned → pending → written, gaps, completion, and abort.
2. Stream tests that alter chunk size and worker count mid-input and verify decompressed bytes exactly match input.
3. Controller boundary tests for invalid sizes/counts.
4. `TestBackend` layout tests for labels, one-based chunk IDs, OFF state, overflow, focus order, and values.
5. CLI integration tests for level 9 default and explicit override.

## Architectural Constraints

- No TUI type may enter `pipeline.rs` or `stream_mode.rs`.
- No chunk may be copied solely for visualization.
- Sequence numbers, not event arrival order, determine output order.
- Maximum worker pool size is fixed at startup; the knob changes eligibility, not OS thread creation.
