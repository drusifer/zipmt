# Split-mode TUI Uplift — Technical Architecture

## Architecture Goal

Lift Split mode to the Stream mode observability standard without changing partitioning, compression ordering, or the output format. The TUI consumes authoritative Split events and owns presentation-only history, paging, and aggregation.

## 1. Explicit Sector Lifecycle

Add a typed `SplitStage` (`Waiting`, `Running`, `Done`) and carry it in Split progress events. `split_mode` emits:

1. `Waiting` when the partition inventory is created.
2. `Running` immediately before a Rayon task starts compression.
3. Progress updates while the task consumes its partition.
4. `Done` only after the compressed byte count is known.

`TuiState::apply_progress_event` remains the single reducer. `StripeProgress` stores the latest stage plus bounded processed, total, and output byte counts. Internal sector IDs stay zero-based; rendering alone converts them to `S01`, `S02`, and so on.

The event stream is observational. It must not introduce new locks, reorder Rayon work, or change sequential output assembly.

## 2. Aggregate Split Model

Pure helpers derive the whole-job view:

- input processed: sum of each sector's processed bytes clamped to its total;
- input total: sum of sector totals;
- output produced: sum of completed sector output bytes;
- active and completed counts: derived from lifecycle stage;
- percentage: bounded aggregate processed / aggregate total;
- elapsed: existing TUI job clock;
- ETA: remaining input / observed aggregate input rate, shown as `--` until the rate is positive.

The aggregate compression ratio uses only completed-sector input bytes divided by completed output bytes. Mixing all processed input with output that is only known at completion would display a misleading transient ratio.

## 3. Shared Mirrored I/O History

Retain the existing fixed-cadence `IoSample` history and RATE/CUMULATIVE toggle. Generalize `sample_io` by mode:

- Stream counters use the existing pipeline cumulative totals.
- Split input is aggregate processed bytes.
- Split output is aggregate completed output bytes.

Each sample stores totals and deltas calculated from actual elapsed duration. Waiting intervals append zero rates. Toggling `I` changes only the selected fields and never clears history. The existing shared-scale mirrored renderer remains the single chart implementation.

## 4. Responsive Sector Board and Paging

Add `split_sector_offset` to TUI state. Rendering computes visible capacity from the actual sector-panel height, clamps the offset, and shows a stable slice. `PgUp` and `PgDn` move by one visible page without affecting compression.

The panel title reports the visible one-based range and overflow, for example `SECTORS S01-S05 of 12 (+7)`. Every row includes `WAIT`, `RUN`, or `DONE`, processed/total bytes, percentage, and output/ratio when complete. Bars consume remaining width after essential text.

At 80x22 the body pairs a compact aggregate/sector panel with the chart. Extra height first increases visible sector rows, then chart detail and logs; extra width expands sector detail and chart history. Geometry is calculated from the current frame, not terminal constants.

## 5. Truthful Mode-aware Controls

Split encoder level is loaded once before each compressor creates its encoder. Since Rayon sectors start concurrently, changing the controller level cannot reliably affect unfinished Split work. Split therefore exposes:

- Pause: live;
- Throttle: live and focusable;
- Level: `ENCODER FIXED`;
- Chunk: `PARTITION FIXED`;
- Workers: `POOL FIXED`.

Mode-aware focus, keyboard, and mouse handling must skip all fixed cards. `[`/`]` are no-ops in Split mode. Stream mode retains its four live controls and existing behavior.

Rendering and hit testing must share the same footer geometry so larger terminals remain aligned.

## 6. Verification

Bounded tests must cover:

- lifecycle reduction and repeated-progress clamping;
- aggregate percentage, counts, ETA fallback, and completed-only ratio;
- one-based labels, paging, offset clamping, and overflow;
- Split RATE/CUMULATIVE samples including idle zero-rate intervals;
- mode-aware Tab, keyboard, and mouse behavior;
- independent Split and Stream snapshots at minimum and expanded sizes;
- byte-perfect Split integration behavior;
- real PTY review at 80x22 and 120x30.

## Constraints

- No runtime repartitioning or Rayon pool resizing.
- No file-format, compressor, or output-order changes.
- No new compression-path synchronization for presentation.
- No regression to Stream controls, charting, or lifecycle display.
