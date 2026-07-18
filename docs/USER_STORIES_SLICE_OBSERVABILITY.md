# Slice Observability and System Telemetry

## Goal

Give Split operators dedicated per-slice and composite metrics with a stable ETA, while exposing process CPU and memory utilization in both TUI modes.

## Stories

1. **Slice widgets:** Each visible slice shows lifecycle, progress, input/output bytes, average input/output throughput, and compression ratio. Paging and one-based identity remain.
2. **Composite metrics:** A distinct aggregate group shows total progress, active/done counts, aggregate bytes, average rates, ratio, elapsed time, and ETA based on stable whole-job average throughput.
3. **System utilization:** Split and Stream show process CPU percentage and resident memory, sampled independently of rendering/input events with graceful unsupported-platform fallback.
4. **Expanded layout:** Body space prioritizes slice/composite status and the mirrored graph. Logs yield vertical space at the minimum, while larger terminals expose more slices and chart history.
5. **Stream worker board:** Stream mode gives each visible worker a stable row with current/last chunk, lifecycle, progress, average throughput, ratio, and ETA, plus explicit overflow.

## Acceptance

- ETA does not use the latest 100 ms rate sample.
- Per-slice averages use each slice's active wall time and freeze at completion.
- Aggregate ETA uses remaining bytes divided by whole-job average input throughput.
- CPU and RSS appear in both modes and render `--` when unavailable.
- 80x22 remains supported; 120x30 materially expands slice visibility.
- Compression, ordering, bounded-memory behavior, and raw telemetry remain unchanged.
