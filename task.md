# Task Board: Slice Observability and System Telemetry

## Phase 1: Stable Slice and Composite Metrics

- [x] **Task 1.1 — Slice timing and averages:** Track authoritative slice start/end times and derive per-slice progress, average input/output rates, and ratio. (Assignee: Neo | UAT: Trin)
- [x] **Task 1.2 — Composite ETA:** Derive aggregate bytes/rates/ratio/counts and ETA from whole-job average throughput rather than the latest sample. (Assignee: Neo | UAT: Trin)

## Phase 2: Shared System Telemetry

- [x] **Task 2.1 — CPU/RSS sampling:** Sample process CPU ticks and resident memory on fixed cadence with parser tests and unsupported-platform fallback. (Assignee: Neo | UAT: Trin)
- [x] **Task 2.2 — Shared System panel:** Render CPU and memory alongside the graph in both Split and Stream. (Assignee: Neo | UAT: Trin | UX: Smith)

## Phase 3: Responsive Slice UX

- [x] **Task 3.1 — Slice widgets and aggregate group:** Render dedicated detailed slice rows and a distinct composite group with truthful units and stable ETA. (Assignee: Neo | UAT: Trin | UX: Smith)
- [x] **Task 3.2 — Responsive validation:** Preserve 80x22, expand at 120x30, update snapshots, and verify bounded-memory/output behavior. (Assignee: Neo | UAT: Trin | UX: Smith)

## Phase 4: Stable Graph Timebase

- [x] **Task 4.1 — One-second buckets:** Decouple graph history from the 100 ms UI/system tick and emit normalized I/O samples once per second. (Assignee: Neo | UAT: Trin)
- [x] **Task 4.2 — Ten-second moving average:** Drive the distinct moving-average trace and RATE labels from the latest ten one-second buckets. (Assignee: Neo | UAT: Trin | UX: Smith)

## Phase 5: Stream Worker Observability

- [x] **Task 5.1 — Worker progress events:** Emit per-worker chunk input/output progress and retain authoritative timing through BUSY, HOLD, and DONE. (Assignee: Neo | UAT: Trin)
- [x] **Task 5.2 — Stream worker board:** Render visible worker range, chunk, progress, average rate, ratio, ETA, and explicit overflow. (Assignee: Neo | UAT: Trin | UX: Smith)

## Phase 6: Compact Stream Worker Cards

- [x] **Task 6.1 — Fixed worker cards:** Give each visible worker a bordered card with fixed status, chunk, average rate, ratio, ETA, and percent positions. (Assignee: Neo | UAT: Trin | UX: Smith)
- [x] **Task 6.2 — Single-cell gauge:** Render worker progress as a one-cell-high gauge and preserve responsive worker overflow. (Assignee: Neo | UAT: Trin | UX: Smith)
- [x] **Task 6.3 — Persistent fixed-point stats:** Place a dedicated fixed-position stats row below each gauge and format rate, ratio, ETA, and progress with two decimal places. (Assignee: Neo | UAT: Trin | UX: Smith)
- [x] **Task 6.4 — Gauge label contrast:** Invert the percentage label to black-on-cyan when progress fill passes beneath it, with a rendered-cell color regression test. (Assignee: Neo | UAT: Trin | UX: Smith)

## Phase 7: Native Multi-series I/O Chart

- [x] **Task 7.1 — Native mirrored datasets:** Replace the custom character matrix with one Ratatui `Chart` containing positive input and negative output Braille line datasets on a shared scale. (Assignee: Neo | UAT: Trin)
- [x] **Task 7.2 — MA and dotted guides:** Add separate magenta MA10s datasets and muted dotted guide datasets, retaining RATE/CUMULATIVE labels and responsive snapshots. (Assignee: Neo | UAT: Trin | UX: Smith)

## Phase 8: Worker Ratio and Chart Separation

- [x] **Task 8.1 — Stable worker ratio:** Mark worker output as finalized explicitly, hide ratio during encoder buffering, and render only a bounded fixed-width final ratio. (Assignee: Neo | UAT: Trin)
- [x] **Task 8.2 — Faint I/O divider:** Render the native chart zero guide as a faint continuous Braille divider while retaining dotted outer guides. (Assignee: Neo | UAT: Trin | UX: Smith)
- [x] **Task 8.3 — Worker ratio moving average:** Retain each worker's last 10 finalized chunk ratios and display their bounded fixed-width average across subsequent assignments. (Assignee: Neo | UAT: Trin)
- [x] **Task 8.4 — Worker rate and ETA smoothing:** Average worker throughput over a 10-chunk window including the active assignment as a provisional newest sample, and drive worker ETA from that rate. (Assignee: Neo | UAT: Trin)

## References

- `docs/USER_STORIES_SLICE_OBSERVABILITY.md`
- `docs/ARCH_SLICE_OBSERVABILITY.md`

## Phase 9: Rust Quality Tooling

- [x] **Task 9.1 — Core quality gates:** Add Make targets for rustfmt, strict Clippy, cognitive complexity, cyclomatic metrics, and dead-code checks. (Assignee: Trin)
- [x] **Task 9.2 — Security and analysis tools:** Add RustSec, cargo-deny, cargo-geiger, Miri, Valgrind, flamegraph, and binary-bloat workflows. (Assignee: Trin)
- [x] **Task 9.3 — Baseline and documentation:** Install Cargo tools, document usage, and record existing quality debt without suppressing findings. (Assignee: Trin)
- [ ] **Task 9.4 — CI enforcement:** Resolve baseline blockers and wire deterministic quality gates into CI. (Assignee: Tank | UAT: Trin)
