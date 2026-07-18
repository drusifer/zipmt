# I/O Flow Chart Follow-on Sprint

## Product Story

As a stream-mode operator, I want a continuously scrolling view of input and output byte flow so that I can see ingestion, compression output, stalls, and changes caused by runtime controls without losing the chunk lifecycle view.

## User-facing Requirements

- Render a mirrored I/O history chart in stream mode:
  - `BYTES IN` occupies the upper half.
  - `BYTES OUT` occupies the lower half.
  - A labeled center baseline separates the two series.
- Provide two visible chart modes:
  - `RATE` converts recent byte deltas and actual sample duration to bytes per second so motion and stalls are obvious.
  - `CUMULATIVE` plots total bytes in and total bytes out over the session.
- Default to `RATE`; pressing `I` toggles the mode, and the chart title must always identify the active mode.
- Use one shared vertical scale for both series so their relative magnitude remains honest.
- Scroll oldest samples left and newest samples right.
- Sample on the existing TUI refresh cadence and retain enough history to fill the available chart width.
- Keep the chart readable without relying on color: labels, baseline, direction, and current values must remain textual.
- Preserve the existing Input Queue → Workers → Pending Sort → Output Ready view and all four runtime controls.
- Preserve the 80x22 minimum layout. Larger terminals should provide a wider and/or taller graph instead of leaving unused space.
- Make the controls footer vertically responsive: retain compact controls at 80x22, then allocate additional rows to taller Level, Throttle, Chunk, and Workers gauges on taller terminals.
- Divide extra vertical space intentionally between chart history, live logs, and taller controls; do not let the log panel consume every additional row.
- Freeze the final history at completion rather than clearing it.

## Acceptance Criteria

1. A deterministic render test shows `BYTES IN` above the center baseline and `BYTES OUT` below it.
2. History tests prove that rate samples normalize deltas to bytes per second, cumulative samples are totals, both are ordered oldest-to-newest and capped to the viewport, and rate mode produces zero when counters do not advance.
3. Both series use the same scale for a given frame.
4. A larger-terminal render contains more horizontal chart samples than the 80-column render.
5. Existing queue-flow labels, one-based chunk IDs, four controls, and minimum-size snapshots remain present.
6. A real PTY stream visibly scrolls both series without flicker or terminal artifacts.
7. `I` switches between `RATE` and `CUMULATIVE` without resetting history or affecting compression, and the active mode is visibly labeled.
8. At 30 terminal rows, each knob has more vertical gauge space than at 22 rows, with keyboard and mouse adjustment aligned to the expanded controls.

## Scope

In scope: stream-mode I/O sampling, rate/cumulative toggle, mirrored rendering, responsive sizing, taller knobs, deterministic tests, and real-terminal UX verification.

Out of scope: persistent telemetry, chart export, split-mode chart redesign, configurable sampling intervals, and moving-average configuration.

## Architecture

### State and Sampling

- Add `IoChartMode::{Rate, Cumulative}` to `TuiState`, defaulting to `Rate`.
- Add a rolling `IoSample` history. Each sample stores:
  - input and output bytes-per-second values calculated from counter deltas and actual elapsed sample time;
  - input total and output total for cumulative mode.
- Track both previous input and previous output counters.
- Extract sampling into a deterministic state method. Sample at a fixed 100 ms deadline rather than once per event-loop iteration, preventing keyboard/mouse activity from changing graph density.
- Cap retained history using terminal width plus a small bound, and let rendering select the newest samples that fit the current viewport. Resizing therefore expands/contracts the visible window without clearing history.
- `I` changes only `io_chart_mode`; compression and sample history are untouched.

### Rendering

- Add a pure mirrored-chart renderer that accepts samples, mode, width, and height.
- Select rate or total fields based on mode, then compute one maximum across both input and output. Use that shared maximum for the upper and lower halves.
- Render newest samples at the right edge:
  - upper half grows upward from the center baseline for input;
  - lower half grows downward from the baseline for output.
- Include textual `BYTES IN`, `BYTES OUT`, current values, units (`/s` in rate mode), and `[I] RATE` or `[I] CUMULATIVE` in the chart block/title.
- In stream mode, split the body horizontally between a compact lifecycle panel and the I/O chart. Width constraints remain responsive; split mode retains its existing status/history presentation.

### Responsive Vertical Geometry

- Compute heights from `extra_rows = terminal_rows - 22`.
- Allocate approximately half of extra rows to the body/chart, up to four rows to the footer/knobs, and the remainder to logs.
- Keep the 22-row allocation at body 10, logs 5, footer 6.
- Render knob gauges from their actual inner height instead of three hard-coded rows.
- Derive mouse mappings from the same footer rectangle and interpolate pointer row across the actual gauge height. This removes the current fixed four-row hit-zone assumption.

### Verification

- Unit-test sampling, history bounds, mode toggling, shared scale, mirrored orientation, and responsive mouse interpolation.
- Update deterministic 80x22 stream snapshots and add a 120x30 snapshot/assertion for additional chart and knob height.
- Run bounded Rust tests through `make test-rust ARGS=...`.
- Finish with a real PTY stream at 80x22 and 120x30, including an `I` toggle, to inspect scrolling stability and terminal cleanup.

### Compatibility and Risk

- No compression-pipeline or file-format changes are required.
- The chart reads existing cumulative `ProgressEvent` counters; it does not add worker synchronization.
- Split-mode behavior is unchanged except for shared responsive knob rendering.
- Primary risks are Unicode width alignment, zero-height halves at minimum size, and stale mouse geometry; pure render/mapping tests cover each boundary.
