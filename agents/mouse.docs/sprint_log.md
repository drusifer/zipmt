# Sprint Log

This document records the historical sprint data for the `zipmt` project.

| Sprint Name | Date Closed | Tasks Completed | Tests Passing | Goal / Scope |
|-------------|-------------|-----------------|---------------|--------------|
| Rust Implementation Sprint | 2026-07-13 | 9 / 9 | 9 | Implement core parallel multi-format compression in Rust (`zipmt-rust`). |
| TUI Sprint | 2026-07-13 | 7 / 7 | 10 | Implement TUI progress visualizer for Split and Stream modes. |
| TUI Testing Sprint | 2026-07-13 | 3 / 3 | 12 | Decouple buffer drawing and implement layout snapshot tests using `insta`. |
| TUI UX Upgrade Sprint | 2026-07-14 | 5 / 5 | 12 | Upgrade TUI with retro diagnostics styling, real-time ETA, and forecasting. |
| TUI LCARS Upgrade Sprint | 2026-07-14 | 7 / 7 | 12 | Support full-screen alternate-screen console, live charts, and pause/throttle hooks. |
| Ratatui Migration Sprint | 2026-07-14 | 9 / 9 | 20 | Migrate TUI to widget-based Ratatui, use crossterm main event loop, and update snapshot tests to `TestBackend`. |
| CLI Defaulting & Fallbacks Sprint | 2026-07-15 | 1 / 1 | 7 | Implement TUI defaulting and TTY auto-redirection/fallback checks. |
| Decoupling & Interactive TUI Upgrade Sprint | 2026-07-15 | 8 / 8 | 7 | Decouple pipeline logic via channel streams, build thread-safe controller, restore -T CLI flags, create Tab-focused vertical sliders with Crossterm mouse click/drag, and update mock snapshot tests. |
| Pipeline Flow Observability and Runtime Controls Sprint | 2026-07-16 | 9 / 9 | 7 | Track chunks across pipeline stages, add safe chunk-size/worker controls, default compression to level 9, and flex the TUI across larger terminals. |
| Mirrored I/O Chart and Responsive Knobs Sprint | 2026-07-17 | 3 / 3 | 7 | Add smooth rate/cumulative mirrored I/O history and taller responsive runtime controls. |
| Split-mode TUI Uplift Sprint | 2026-07-17 | 9 / 9 | 7 | Add authoritative sector lifecycle, aggregate status, responsive paging/chart parity, and truthful fixed controls. |
| TUI Completion and Smoothed I/O Sprint | 2026-07-17 | 4 / 4 | 7 | Retain successful completion dashboards with final statistics and add five-sample moving-average RATE overlays. |
| Bounded-memory Split Streaming Sprint | 2026-07-17 | 4 / 4 | 7 | Seek source ranges, stream to destination-adjacent temporary sections, concatenate in order, and expose both output-write phases. |
| Slice Observability and System Telemetry Sprint | 2026-07-17 | 6 / 6 | 7 | Add per-slice widgets, composite ETA/metrics, and shared CPU/RSS panels. |
| Graph Timebase and Stream Worker Sprint | 2026-07-17 | 4 / 4 | 7 | Add one-second graph buckets, ten-second smoothing, and per-worker Stream progress/ETA. |
| Compact Stream Worker Cards Sprint | 2026-07-17 | 4 / 4 | 7 | Add bordered worker cards, fixed-point telemetry rows, and accessible gauge-label contrast. |
| Native Multi-series I/O Chart Sprint | 2026-07-17 | 2 / 2 | 7 | Replace custom graph text with signed Braille Chart datasets, MA10s overlays, and dotted guides. |
| Worker Ratio and Chart Separation Sprint | 2026-07-17 | 4 / 4 | 7 | Add 10-chunk worker rate/ratio smoothing, stable ETA, and a faint native center divider. |
