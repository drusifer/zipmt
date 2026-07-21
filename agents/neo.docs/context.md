# Agent Local Context (context.md)

- **Rust boundary refactor sprint complete (2026-07-20):** Stream and Split
  orchestration have named roles; controller atomics are private behind
  snapshots/validated commands; progress is explicitly best-effort; codecs
  share one allocation-preserving control loop; the application shell uses a
  typed plan and output guard. All deterministic gates pass. Alternating 32 MiB
  XZ benchmarks improved 1.15% versus pre-sprint HEAD.
- **Longitudinal benchmark ledger (2026-07-20):** `make rust-benchmark` reuses
  `build/rust-benchmark-input.bin` and appends YAML records keyed by revision,
  dirty state, workload hash, configuration, mean time, and throughput. The
  retained ledger is `benchmarks/rust-history.yaml`.
- **Rust Refactor 3 implementation (2026-07-20):** Dashboard panels are named
  renderers behind a bounded coordinator; chart preparation and Ratatui
  composition are separate; terminal lifetime is RAII-owned; runtime polling,
  draining, ticking, and joining are separate roles; keyboard commands are
  family-dispatched with shared mouse setters; startup resolution, signal
  installation, cleanup, and exit policy are typed services.

This file tracks the current state of code implementations and tech stacks maintained by the Software Engineer (Neo).

## Recent Decisions
- **Phase 1 complexity fix and PTY automation (2026-07-18)**: Runtime event policy is split into bounded key/mouse/control helpers and rendering is split into bounded panel closures. TUI no longer appears in the complexity gate. Added `make rust-pty-smoke`; real PTY passes at 80x22 and 120x30 with output integrity and terminal cleanup.
- **TUI platform/runtime/render boundaries and zero Clippy baseline (2026-07-18)**: Added platform, runtime, and render modules; isolated terminal lifecycle and simplified platform fallbacks. Full tests pass and strict Clippy is now clean. Phase 1 remains open because run/draw still exceed cognitive complexity and require a focused fix loop.
- **Pure reducer and body layout profile (2026-07-18)**: Progress transitions now live in `tui/reducer.rs`, accept an injected Instant, perform no I/O, and return a process-sampling request for the runtime wrapper. Rendering consumes a pure mode-to-body-layout profile. Terminal polling/widgets remain unmoved for Task 1.3.
- **Typed TUI mode and worker lifecycle (2026-07-18)**: `ModeState::{Stream, Split}` replaces `TuiMode`, and `WorkerStage::{Off, Idle, Busy, Hold}` now flows through ProgressEvent, Stream producers, TUI reduction, cleanup, rendering labels, and tests. String lifecycle state is removed without moving reducer/runtime/rendering boundaries.
- **Phase 0 I/O characterization (2026-07-18)**: All four I/O pairings now have real-binary equivalence evidence. A Linux 128 MiB File-to-Stdout regression requires peak RSS below 96 MiB and validates the decoded zero stream. SIGINT against stdin-to-file Stream must exit 2 and remove its incomplete output. Existing strict Clippy/complexity failures are confined to the recorded baseline, not Phase 0 changes.
- **Bounded File-to-Stdout streaming (2026-07-18)**: File input destined for stdout is now opened and passed directly to `compress_stream`; the prior whole-input `std::fs::read` and cursor are removed. A focused Gzip file-to-stdout decompression test passes, and the Make wrapper now forwards `test-rust ARGS` for bounded test execution.
- **MLflow-style Judge efficiency rubric (2026-07-18)**: Judge deterministically flags exact duplicate tools, unchanged immediate retries, tests repeated without edits, repeated searches without scope/edit changes, and existing duplicate reads. Stateful polling tools are exempt. Reports emit a YES/NO efficiency verdict and specific findings.
- **Judge shell classification fix (2026-07-18)**: Quote-aware shell segmentation now evaluates only executable positions, distinguishes `|` from `||`, and confines Via heuristics to each `rg`/`grep` segment. Full-session flags fell from 9 to the single confirmed Via violation.
- **Unified chunk-buffer reuse (2026-07-18)**: Stream transfers its reader allocation without cloning and feeds the chunk directly to encoders. Split workers reuse one heap chunk, replacing it only at atomic Chunk-size read boundaries; encoder output still writes directly to temporary slice files. Bounded verbose logs expose worker range, chunk replacement, 10% milestones, temp output, completion, and final concatenation.
- **Scrollable TUI logs (2026-07-18)**: Logs follow the tail by default; unfocused Up/Down and mouse wheel move through history, while Home/End jump to oldest/newest. Scroll-back is clamped to retained history.
- **Native Split/Stream profiles (2026-07-18)**: Symbolized two-worker XZ level-1 perf runs attribute 87.29% Split and 87.90% Stream self-cost to four liblzma match-finder/encoder symbols; Rust orchestration, direct chunk input, ordering, and I/O do not appear as material self-costs.
- **Smoothed worker rate and ETA over 10 chunks (2026-07-17)**: Each worker keeps 10 finalized input rates; an active assignment joins as the provisional newest sample, and the same average drives both AVG and ETA.
- **Added per-worker ratio moving average (2026-07-17)**: Each Stream worker retains its last 10 finalized chunk ratios; the fixed-width average remains visible across assignments and adapts as chunks complete under new compression levels.
- **Stabilized worker ratio and chart separation (2026-07-17)**: Worker progress now distinguishes final output, hides ratio during encoder buffering, and bounds the fixed final field; the native chart zero guide is a faint continuous Braille divider.
- **Implemented native multi-series I/O chart (2026-07-17)**: Replaced the custom character matrix with Ratatui Chart datasets: signed cyan/yellow raw Braille lines, magenta MA10s lines, and muted dotted scatter guides on one mirrored scale.
- **Fixed worker gauge percentage contrast (2026-07-17)**: Gauges now explicitly use cyan foreground and black background, allowing filled percentage labels to invert to black-on-cyan; a >50% rendered-cell test locks the behavior.
- **Separated Stream gauges and stats (2026-07-17)**: Worker cards now put the one-cell progress gauge above an always-visible stats row; rate, ratio, ETA, and percent use fixed-width two-decimal formatting.
- **Implemented compact Stream worker cards (2026-07-17)**: Each visible worker uses a three-row bordered card with identity/chunk in the title and a one-cell gauge labeled with fixed AVG, ratio, ETA, and percent fields.
- **Implemented graph buckets and Stream worker board (2026-07-17)**: Graph history uses one-second buckets and MA10s; Stream worker assignment state exposes progress, average input rate, ratio, ETA, and retained DONE metrics.
- **Implemented slice/system observability (2026-07-17)**: Slice lifetime averages freeze independently; composite ETA uses whole-job average throughput; both modes show fixed-cadence process CPU/RSS beneath the chart.
- **Implemented bounded Split streaming (2026-07-17)**: Workers seek independent ranges and stream through 64 KiB buffers into destination-adjacent auto-cleaned temporary files; ordered concatenation is also buffered. Live temp output and final destination writes feed distinct truthful telemetry.
- **Implemented persistent completion and smoothed I/O (2026-07-17)**: Successful interactive TUIs freeze final telemetry and wait for Enter/Q/Esc; automation exits. RATE uses a five-sample moving-average overlay and labels while CUMULATIVE remains exact.
- **Implemented Split TUI uplift (2026-07-17)**: Typed Waiting/Running/Done events feed bounded aggregate helpers and shared I/O samples. Split renders a responsive one-based paged lifecycle board and mirrored chart; only Throttle is focusable, while Level/Partition/Pool are visibly fixed.
- **Mirrored I/O chart implementation (2026-07-17)**: Stream TUI samples input/output at a fixed 100 ms cadence into one history containing bytes-per-second rates and cumulative totals. `I` toggles views without reset. The chart uses a shared mirrored scale; responsive rows are divided across body/logs/footer; knobs and mouse mappings use actual footer height.
- **Responsive TUI geometry (2026-07-16)**: Terminals at or above 80x22 now use the full available canvas. The log panel absorbs extra height, stream/split bodies expand horizontally, header rules scale to preserve the complete status label, footer controls stay right-anchored, and mouse hit zones follow the resized footer.
- **Pipeline Flow Phase 3 (2026-07-16)**: Stream mode uses the full dashboard body for labeled pipeline flow; overflow is explicit; four equal footer cards expose Level, Throttle, Chunk, and Workers. Focus order follows visual order, mouse maps all cards, and live controller values are visible.
- **Pipeline Flow Phase 2 (2026-07-16)**: Added atomic power-of-two chunk sizing and fixed-pool active-worker eligibility. Reader block boundaries load the current size per future block; disabled workers never interrupt in-flight work and requeue untouched jobs on an eligibility race. A reader-done signal prevents sender clones from keeping workers alive after EOF.
- **Pipeline Flow Phase 1 (2026-07-16)**: Added shared `DEFAULT_COMPRESSION_LEVEL = 9`, initial-level pipeline construction to eliminate the first-chunk race, typed chunk lifecycle events at reader/worker/writer authority points, and one `TuiState::apply_progress_event` reducer that keeps queue/worker/pending stages mutually exclusive.
- **One-based chunk display correction (2026-07-16)**: Internal stream sequence IDs remain zero-based for ordering; every operator-facing queue, worker, pending, next, and gap label displays `seq + 1` and is covered by a focused render test.
- **Fixed stdin-streaming TUI eligibility (2026-07-16)**: TUI rendering owns stderr, so eligibility now checks whether stderr is a terminal rather than requiring piped stdin and compression stdout to be terminals. `-T` therefore works with `cat input | zipmt-rust -T -o output -`; redirected stderr still safely falls back. Added a pure selector with four regression tests and made `--no-tui` an explicit override.
- **Implemented LCARS Dashboard**: Added full-screen grids and rolling history sparkline graph.
- **Implemented Dynamic Throttling**: Added keyboard control event listeners (`+`/`-`/`p`/`q`).
- **Fixed Stream Crash & Added Centering**: Prevented `usize` subtraction underflows inside the progress bar rendering loops and added horizontal/vertical screen padding using crossterm size queries.
- **Standardized Border Alignment**: Formatted all dashboard panels to exactly 80 characters wide for straight vertical borders.
- **Implemented Safe Sizing & Resize Event Tracking**: Added fallback environment checks, command queries (`tput cols`/`lines`), and crossterm resize listeners to correctly center output even when stdout is redirected (e.g. `> /dev/null`).
- **Added Redirection Sizing Fixes & TDD Coverage**: Used `/dev/tty` and `/dev/stderr` standard descriptor streams to query terminal boundaries via `stty size` under redirected pipe conditions.
- **Moved TUI Loop to Main Thread**: Spawns compression process on a background thread and runs Crossterm event/raw mode logic on the main thread for optimal signal/stream control.
- **Aligned Section Headers**: Audited and padded Split Mode and Stream Mode header dividers to perfectly align at 80 characters.
- **Aligned Progress Bars**: Padded Split Mode progress bar layout from 39 characters down to exactly 38 characters for perfect alignment of the middle vertical divider `│`.
- **Phase 1 of Ratatui Migration**: Completed Task 1.2 and Task 1.3. Removed the `-T`/`--tui` command line flag from main, defaulting to TUI mode with fallback checks for non-TTY or stdout redirection. Initialized the `stderr` alternate screen raw mode using the `ratatui::Terminal` inside `TerminalGuard`.
- **Phase 2 of Ratatui Migration**: Completed Task 2.1, 2.2, and 2.3. Replaced the non-blocking polling and manual thread sleep loop with main-thread event loop polling (`crossterm::event::poll(tick_rate)`) at a tick rate of 100ms, draining all events per iteration to maintain responsiveness and keep the worker compression thread and user inputs properly synchronized.
- **Phase 3 of Ratatui Migration**: Completed Task 3.1, 3.2, and 3.3. Re-rendered all LCARS dashboard layout panels and components (System Status, Sectors progress list, Transporter buffer capacity, rolling speed history graph, and controls panel) using Ratatui widgets (`Paragraph`, `Line`, `Span`, `Style`) and `Layout` constraints centered dynamically on the screen. Migrated layout snapshot tests to use Ratatui's `TestBackend` buffer cell symbol assertions instead of raw string ANSI stripping, ensuring clean, pixel-perfect formatting validation.
- **TUI UX Restructuring & Knobs**: Implemented real-time average chunk compression duration tracking, dynamic compression level knobs (`[` and `]`), and restructured layout from raw row rendering to native Ratatui rounded border blocks and structured sub-panels (Title, Body splits, Logs, and Footer).
- **TUI Defaulting and Fallbacks (Task 1.2)**: Implemented robust auto-redirection/TTY fallback checks for TUI mode, removed the manual `-T`/`--tui` flag, and updated test suite triggers with `ZIPMT_FORCE_TUI=1`.
- **Modular Pipeline & Decoupling (R1 & R2)**: Fully decoupled compression execution from terminal rendering. Introduced event-based channel logging and progress updates via `ProgressEvent`. Encapsulated concurrency and execution within the thread-safe `CompressionPipeline` and `PipelineController`.
- **Smart CLI Controls & Sliders (R3 & R4)**: Restored `-T`/`--tui` and added `--no-tui` CLI arguments. Implemented focusable, responsive vertical slider blocks for compression level and throttle delay inside the LCARS panel footer, complete with crossterm mouse clicking/dragging event handling and keyboard Tab key widget focusing.
- **CLI Opt-In TUI Bug Fix (R3)**: Resolved TUI defaulting issue. Made TUI strictly opt-in via `-T`/`--tui` with non-TTY/redirection override fallback to false, and environment override `ZIPMT_FORCE_TUI` to true. Updated integration tests to verify opt-in and override logic.

---
*Last updated: 2026-07-18T20:15:00-04:00*
