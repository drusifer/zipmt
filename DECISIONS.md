Architectural Decision Record (ADR) detailing design choices for zipmt.

TLDR:
    Impact: Captures structural decisions (GLib, file-deletion by default, static splitting, stream throttling, Ratatui UI library, main event loop).
    Next Steps: Document lessons learned from these choices.

# Architectural Decisions Record (DECISIONS.md)

This document records the key architectural and design decisions made during the development of `zipmt`.

---

## 1. Decision: Use of GLib 2.0 for Threading & Concurrency
- **Date:** Historical
- **Status:** Approved
- **Context:** The utility requires a robust, platform-independent concurrency model to perform parallel compression. Implementing raw POSIX threads (`pthread`) requires building custom thread pool and queue mechanisms.
- **Decision:** Utilize GLib 2.0 (`GThreadPool`, `GMutex`, `GSList`, and memory slices).
- **Rationale:** GLib provides mature, tested abstractions for thread pools and thread-safe data structures, reducing the complexity of custom queue management.
- **Consequences:** 
  - Adds dependency on GLib 2.0 runtime and development packages (`libglib2.0-dev`).
  - Limits compiler target platform compatibility to environments where GLib is available.

---

## 2. Decision: Default Deletion of Source Files
- **Date:** Historical
- **Status:** Approved
- **Context:** Standard Unix compression utilities (`gzip`, `bzip2`) delete the source file after compression unless instructed otherwise (e.g., via piping to stdout).
- **Decision:** Delete the input file by default upon successful compression.
- **Rationale:** Mimic the UX expectations of CLI compression tools.
- **Consequences:**
  - **High Danger:** Risk of accidental data loss if developers run `zipmt` without realizing it deletes the source file.
  - Requires the implementation of a keep flag `-k` / `--keep` to override this behavior, and prominent warnings in documentation.

---

## 3. Decision: Static File-Splitting (Split Mode)
- **Date:** Historical
- **Status:** Approved
- **Context:** Compressing large files on disk needs to be parallelized with minimal lock contention between threads.
- **Decision:** Partition the file size statically by the number of threads: `fileSize / nthreads`. Each thread writes to its own temporary file (`.tmp<n>`), and the main thread concatenates them sequentially at the end.
- **Rationale:** Eliminates the need for thread-to-thread communication or synchronization during compression. Threads run independently at 100% core utilization.
- **Consequences:**
  - Produces intermediate temp files on disk, increasing disk write/read cycles.
  - Concatenation step is sequential and limited by single-threaded disk I/O at the end of the run.

---

## 4. Decision: Throttling Stream Reads
- **Date:** Historical
- **Status:** Approved
- **Context:** In Stream Mode, the file reader thread can read blocks from stdin/disk much faster than compression threads can process them. Without limits, the reader would load the entire input into memory, causing out-of-memory (OOM) crashes on large files.
- **Decision:** Throttle the reader loop by halting new block reads if the number of pushed-but-incomplete parts exceeds `nthreads * 2`.
- **Rationale:** Restricts peak memory consumption to a predictable maximum of `READBUFZ * nthreads * 2` (approx. 8MB per thread).
- **Consequences:**
  - Maintains stable, low-footprint memory usage during long-running stream operations.

---

## 5. Decision: Go Concurrency via Goroutines & Channels
- **Date:** 2026-07-13
- **Status:** Approved
- **Context:** Parallelizing stream compression in Go requires thread-safe task distribution and response gathering.
- **Decision:** Use unbuffered/buffered channels (`jobs` and `results`) and goroutines instead of explicit thread pools and mutex locks.
- **Rationale:** Aligns with Go's CSP model ("Do not communicate by sharing memory; instead, share memory by communicating"). It eliminates locks and potential race conditions during task routing.
- **Consequences:**
  - Code is simpler, more readable, and easier to scale.
  - Channels introduce minor routing overhead, which is negligible compared to compression computation.

---

## 6. Decision: Go In-Memory Reordering via Pending Parts Map
- **Date:** 2026-07-13
- **Status:** Approved
- **Context:** Compressed parts arrive out of order because compression duration varies per block. Writing them out of order corrupts the output file.
- **Decision:** The Go version routes all results to a single `writeWorker` goroutine. This worker maintains a `pending_parts` hash map (`map[int]*ZipPart`). If a block arrives out of order, it is cached in the map. When the expected sequence number arrives, it is written, and the map is checked iteratively for subsequent parts.
- **Rationale:** Keeps write operations single-threaded to avoid multi-thread write contention and disk head thrashing, while avoiding complex mutex-locked priority queues.
- **Consequences:**
  - Storing uncompressed/compressed parts in the map increases memory consumption temporarily if block compression times diverge significantly.

---

## 7. Decision: Migration to Ratatui Widget-Based TUI Rendering
- **Date:** 2026-07-14
- **Status:** Approved
- **Context:** The original C and Go versions used custom ANSI-escaped terminal output. This approach is fragile, hard to maintain, layout calculations are manual and prone to errors, and rendering is difficult to unit/snapshot test reliably.
- **Decision:** Migrate the Rust TUI implementation to use the `ratatui` library, utilizing its `Layout` constraints, `Rect` math, and standard widget rendering models (e.g., `Paragraph`, `Line`, `Span`, `Style`) to represent the retro LCARS console.
- **Rationale:** Ratatui provides a clean, declarative layout engine and component hierarchy. This separates rendering logic from terminal handle control, improves reliability on resized/constrained terms, and integrates nicely with snapshot test backends.
- **Consequences:**
  - Standardizes UI rendering on a modern, well-supported Rust TUI framework.
  - Simplifies adding new panels, status widgets, and styling without manual string calculations.
  - Requires adding the `ratatui` dependency to `Cargo.toml`.

---

## 8. Decision: Crossterm Main-Thread Event Polling & Draining Loop
- **Date:** 2026-07-14
- **Status:** Approved
- **Context:** Listening to user input (e.g., pause, speed adjustment, or abort keys) originally required spawning a separate background input thread. This creates synchronization complexities, multi-threading overhead, and makes clean terminal teardown on abort difficult.
- **Decision:** Integrate a crossterm-based event loop on the main thread using `event::poll` with a tick rate of 100ms, and an inner loop to drain all immediately pending events at a duration of 0ms.
- **Rationale:** Event polling directly on the main thread simplifies keyboard input handling and terminal resizing. The non-blocking inner event draining loop ensures that multiple rapid keystrokes or resize events are processed instantly without accumulating queue lag.
- **Consequences:**
  - Eliminates the secondary keyboard input listener thread, simplifying thread synchronization.
  - Ensures clean exit handling (e.g., Ctrl+C or abort keys) because the main thread can trigger cleanup and exit immediately.
  - Coordinates with the compression threads through lightweight, atomic synchronization variables (`IS_PAUSED`, `THROTTLE_DELAY_MS`).

---

## 9. Decision: CLI TUI Defaulting & Auto-Redirection Fallback Checks
- **Date:** 2026-07-15
- **Status:** Approved
- **Context:** Running CLI utilities requires sensible defaults. Requiring users to specify `-T`/`--tui` is cumbersome, but launching a full-screen TUI when output is redirected (to a file, pipe, or script) corrupts target streams and degrades CLI interoperability.
- **Decision:** Remove the `-T`/`--tui` CLI flag. Run in TUI mode by default for normal compression, and dynamically disable TUI mode (falling back to standard stream/raw output) if stdout is redirected (via `-c` flag or pipe redirection), if stdin/stdout are not interactive terminals (`std::io::IsTerminal`), or if stdin is redirected when reading from stdin.
- **Rationale:** Ensures clean pipes for automated workflows while maintaining an interactive dashboard experience for users executing the tool directly in a terminal.
- **Consequences:**
  - Reduces interface surface complexity by removing a redundant flag.
  - Automatically prevents data stream corruption during shell piping.
  - Requires the `std::io::IsTerminal` trait from the Rust standard library (available in Rust 1.70+).

---

## 10. Decision: Unidirectional Channel-Based Progress Decoupling
- **Date:** 2026-07-15
- **Status:** Approved
- **Context:** Tight coupling of `TuiState` (via `Arc<Mutex<TuiState>>`) within parallel chunk worker pools and main loops creates synchronization bottlenecks and prevents headless testing or programmatic execution of the compression pipeline as a library.
- **Decision:** Extract compression loops into an independent library module (`pipeline.rs`) and run execution on a separate worker thread. Expose progress indicators via a unidirectional `mpsc::Sender<ProgressEvent>` channel and runtime adjustments via a cloneable `PipelineController`.
- **Rationale:** Decoupling rendering from processing prevents frame drawing from slowing down compression, provides a clean API seam for testing, and encapsulates execution details.
- **Consequences:**
  - Standardizes data flow to unidirectional channel events (`ProgressEvent`), eliminating raw mutex sharing across workers.
  - Enables full test coverage of layout rendering using mock state indicators on `TestBackend` without running compression threads.

---

## 11. Decision: Interactive Vertical Knobs Sliders & Crossterm Mouse Integration
- **Date:** 2026-07-15
- **Status:** Approved
- **Context:** Custom keystroke adjustments (such as global `+`/`-` or `[`/`]`) do not scale well when managing more than two parameters, and lack visual feedback indicating focus or levels resembling LCARS dials.
- **Decision:** Restructure the controls footer panel to render interactive vertical sliders with Tab-focus borders. Integrate Crossterm mouse capture to calculate click coordinates and directly adjust parameters in real-time.
- **Rationale:** Vertical bars resemble Star Trek console reticles, offering highly refined tactile feedback. Mouse capture dramatically increases the retro-futuristic usability.
- **Consequences:**
  - Tab cycles widget focus cleanly between Compression Level and Throttle delay sliders.
  - Requires handling `Event::Mouse` and converting mouse click coordinates to slider intervals.

---

## 12. Decision: CLI Opt-In TUI Restoration and Smart Fallback Override
- **Date:** 2026-07-15
- **Status:** Approved
- **Context:** While defaulting to TUI was convenient, it violated traditional UNIX command line expectations where TUIs are strictly opt-in.
- **Decision:** Restore the `-T` / `--tui` CLI flag. Run in TUI mode *only* if `-T` is specified and TTY checks are satisfied, while supporting an override environment variable (`ZIPMT_FORCE_TUI=1`) to bypass stdout checks for automated snapshot testing.
- **Rationale:** Aligns the tool's behavior with standard CLI design standards while preserving automated test infrastructure.
- **Consequences:**
  - Restores traditional command line UX.
  - Avoids corrupting stdout when `-T` is omitted during pipes.

---

## 13. Decision: Authoritative Chunk Lifecycle Events and One-Based Projection
- **Date:** 2026-07-16
- **Status:** Approved
- **Context:** Operators need to follow individual stream chunks through input queuing, worker assignment, pending output sorting, and ordered writing without coupling the TUI to pipeline internals.
- **Decision:** Emit typed lifecycle events at the reader, worker, and writer transition points, and reduce them through one `TuiState` event handler. Keep sequence IDs zero-based internally but display every chunk as one-based `#N`.
- **Rationale:** Authoritative events prevent contradictory stage displays, while the projection boundary preserves ordering math and presents operator-friendly identities.
- **Consequences:**
  - A chunk is projected in only one lifecycle stage at a time.
  - Output ordering remains unchanged and byte-safe.
  - New frontends can consume the same event vocabulary.

---

## 14. Decision: Future-Only Chunk Sizing and Fixed-Pool Worker Gating
- **Date:** 2026-07-16
- **Status:** Approved
- **Context:** Chunk size and concurrency must be adjustable during streaming without interrupting in-flight compression or destabilizing worker identity and output order.
- **Decision:** Store validated chunk size and active-worker count in atomics. The reader loads chunk size before each future block; a fixed startup worker pool checks eligibility before accepting new work, while already-running chunks finish normally.
- **Rationale:** Future-only application gives controls predictable boundaries. Fixed-pool gating avoids thread churn and preserves stable worker slots in the TUI.
- **Consequences:**
  - Chunk size is restricted to powers of two from 64 KiB through 8 MiB.
  - Active workers are restricted to 1 through the startup pool maximum.
  - Mid-stream changes preserve byte-perfect decompression and ordered output.

---

## 15. Decision: Full-Canvas Responsive TUI with a Stable Minimum Contract
- **Date:** 2026-07-16
- **Status:** Approved
- **Context:** A fixed centered 80x22 dashboard wastes larger terminals, but the minimum layout and mouse controls must remain deterministic.
- **Decision:** Treat 80x22 as the minimum contract and use the complete canvas above that size. Extra rows expand logs, body panels scale horizontally, status rules derive from actual label width, and the four fixed-width control cards and mouse targets remain anchored to the right/bottom edges.
- **Rationale:** Operators gain useful information density on larger terminals without learning a different layout or losing minimum-size compatibility.
- **Consequences:**
  - Existing 80-column snapshots remain stable.
  - Larger terminals expose more logs and stage separation.
  - RUNNING, PAUSED, and COMPLETE remain fully visible at minimum width.

---

## 16. Decision: Dual-mode Mirrored I/O History
- **Date:** 2026-07-17
- **Status:** Approved
- **Context:** Stream operators need smooth short-term I/O motion and session-total growth without losing lifecycle visibility or restarting telemetry when changing views.
- **Decision:** Sample existing cumulative input/output counters on a fixed 100 ms cadence into records containing both normalized bytes-per-second rates and cumulative totals. Render input above and output below one shared baseline/scale; `I` switches RATE/CUMULATIVE projections without clearing history.
- **Rationale:** One sample buffer keeps both views temporally aligned, fixed cadence avoids event-driven distortion, and shared scaling makes input/output magnitude comparable.
- **Consequences:**
  - The feature is isolated to TUI state and rendering.
  - Larger terminals expose more chart history and taller controls.
  - Compression synchronization, ordering, and file formats are unchanged.

---

## 17. Decision: Split Lifecycle Projection and Truthful Fixed Controls
- **Date:** 2026-07-17
- **Status:** Approved
- **Context:** Split mode exposed zero-based progress rows and controls that appeared live even though partitions, Rayon pool size, and encoder level are fixed when work starts.
- **Decision:** Emit typed Waiting/Running/Done events at authoritative Split transitions and reduce them into a bounded aggregate plus one-based paged sector board. Reuse the mirrored I/O history with aggregate Split counters. Present Level as encoder-fixed, Chunk as partition-fixed, and Workers as pool-fixed; only Pause and Throttle remain live.
- **Rationale:** Explicit lifecycle and completed-sector-only ratio provide trustworthy status, while removing false affordances prevents operators from believing startup topology changed.
- **Consequences:**
  - Split paging derives from the same responsive row capacity used by rendering.
  - Aggregate output and ratio advance only when sector output is authoritative.
  - Rayon execution, sequential output assembly, compressor semantics, and file format remain unchanged.

---

## 18. Decision: Persistent Completion Dashboard and Smoothed Rate Projection
- **Date:** 2026-07-17
- **Status:** Approved
- **Context:** Successful jobs immediately closed the TUI, hiding final results, while instantaneous 100 ms rate labels jumped too aggressively for comfortable reading.
- **Decision:** Freeze elapsed time and telemetry on successful completion, retain the interactive dashboard until Enter/Q/Esc, and show final bytes, ratio, and average throughput. In RATE mode, overlay a five-sample moving average and use its latest values for IN/OUT labels; preserve exact CUMULATIVE totals and raw stored samples.
- **Consequences:**
  - Interactive users can inspect final results without racing terminal teardown.
  - `ZIPMT_FORCE_TUI=1` automation renders completion but exits without waiting.
  - Smoothing changes presentation only, not sampling cadence or compression behavior.

---

## 19. Decision: Seeked Split Ranges with Temporary Compressed Sections
- **Date:** 2026-07-17
- **Status:** Approved
- **Context:** Split mode loaded the entire source and retained every compressed slice in RAM, making memory proportional to file size.
- **Decision:** Derive ranges from file metadata. Each worker independently opens and seeks the source, reads its bounded range through a 64 KiB buffer, and compresses directly to an auto-cleaned temporary file. After all workers finish, concatenate temporary streams in range order through a fixed buffer.
- **Consequences:**
  - Application memory is O(workers × fixed buffers), not O(input + output).
  - Encoder-to-temp output is visible during RUN.
  - Final destination writes are added to total output I/O, while compression ratio uses compressed-section bytes only.
  - Temporary sections are removed automatically when their handles drop.

---

## 20. Decision: Slice-local Averages, Composite ETA, and Shared Process Telemetry
- **Date:** 2026-07-17
- **Status:** Approved
- **Decision:** Record authoritative start/completion instants in each TUI slice projection. Show per-slice average input/output throughput and ratio, and calculate composite ETA from total processed bytes over whole-job elapsed time rather than the latest 100 ms sample. Sample process CPU ticks and RSS on the fixed telemetry cadence and render one shared Process panel in both modes.
- **Consequences:**
  - Slice metrics freeze independently at completion.
  - ETA is stable across short encoder flushes and concatenation bursts.
  - Linux reports process CPU/RSS from `/proc`; unsupported platforms show `--`.
  - CPU may exceed 100% when the process consumes multiple cores.

---

## 21. Decision: One-second Graph Buckets and Ten-second Rate Smoothing
- **Date:** 2026-07-17
- **Status:** Approved
- **Decision:** Decouple graph sampling from the 100 ms UI/process telemetry tick. Emit one normalized I/O history bucket per second and calculate the RATE overlay and axis labels from the latest ten buckets.
- **Consequences:**
  - Every horizontal graph step represents approximately one second.
  - `◆MA10s` has a stable, explicit ten-second horizon.
  - Cumulative totals, final statistics, CPU/RSS cadence, and compression events remain unchanged.

---

## 22. Decision: Stream Worker-local Progress Projection
- **Date:** 2026-07-17
- **Status:** Approved
- **Decision:** Emit worker/chunk progress from Stream compressor callbacks, retain per-assignment timing and bytes in `WorkerState`, and render a responsive worker board with one-based chunk, lifecycle, percentage, average input rate, ratio, ETA, and explicit overflow.
- **Consequences:**
  - Worker metrics remain visible as DONE after the worker returns idle.
  - Small terminals show a bounded worker range; larger terminals expose more rows.
  - Stream ordering, queues, compression buffers, and worker gating remain unchanged.

---

## 23. Decision: Compact Stream Worker Cards
- **Date:** 2026-07-17
- **Status:** Approved
- **Decision:** Render each visible Stream worker in a three-row bordered card. Put identity, lifecycle, and chunk in the title; use the single interior row as a progress gauge with fixed average-rate, ratio, ETA, and percentage fields.
- **Consequences:**
  - The minimum 80x22 dashboard retains two complete worker cards.
  - Taller terminals add cards in deterministic three-row increments.
  - Gauge fill supplies progress without shifting worker-specific statistics.

---

## 24. Decision: Separate Worker Gauge and Fixed-point Statistics
- **Date:** 2026-07-17
- **Status:** Approved
- **Decision:** Reserve the first card row for the progress gauge and the second for always-visible statistics. Format progress, rate, ratio, and ETA to two decimal places with fixed field widths.
- **Consequences:**
  - Gauge fill can no longer obscure or recolor worker statistics.
  - Values update in place without changing neighboring field positions.
  - Four-row cards trade minimum-terminal density for clearer telemetry.

---

## 25. Decision: Explicit Gauge Contrast Pair
- **Date:** 2026-07-17
- **Status:** Approved
- **Decision:** Give Stream worker gauges an explicit cyan foreground and black background so Ratatui inverts percentage labels to black-on-cyan beneath the filled region.
- **Consequences:**
  - Percentage text remains readable above and below 50% progress.
  - A rendered-cell regression test verifies the filled-label foreground and background colors.

---

## 26. Decision: Native Multi-series Ratatui I/O Chart
- **Date:** 2026-07-17
- **Status:** Approved
- **Decision:** Replace the custom block/diamond text renderer with one native Ratatui `Chart`. Plot input as positive cyan Braille lines, output as negative yellow Braille lines, MA10s as magenta Braille lines, and dotted guides as muted scatter datasets.
- **Consequences:**
  - Stream and Split share one native multi-series renderer and shared mirrored scale.
  - Braille provides 2x4 sub-cell resolution without a second stacked chart.
  - The existing one-second buckets, cumulative values, and MA10s calculations are unchanged.

---

## 27. Decision: Final-only Worker Compression Ratio
- **Date:** 2026-07-17
- **Status:** Approved
- **Decision:** Add an explicit `finalized` bit to worker progress. Keep ratio unavailable while the encoder may be buffering, then show the exact final ratio in a six-character, two-decimal field capped with `>99.99`.
- **Consequences:**
  - Header-only intermediate output can no longer create giant transient ratios.
  - Worker progress and ETA remain live.
  - The ratio column never shifts neighboring fields.

---

## 28. Decision: Native Zero-line I/O Divider
- **Date:** 2026-07-17
- **Status:** Approved
- **Decision:** Draw the native Chart zero guide as a faint continuous Braille line, with the ±50% guides remaining dotted scatter series.
- **Consequences:**
  - Input and output halves read as distinct plots without separate chart axes.
  - All graph layers remain native Chart datasets.

---

## 29. Decision: Per-worker Ten-chunk Ratio Average
- **Date:** 2026-07-17
- **Status:** Approved
- **Decision:** Each Stream worker retains a rolling window of its last 10 finalized chunk compression ratios. Display the arithmetic mean during later assignments and append only authoritative final encoder results.
- **Consequences:**
  - `R` remains visible instead of resetting to `--.--` for every new chunk.
  - Compression-level changes affect the average as newly configured chunks complete.
  - Partial buffered output cannot distort the ratio.

---

## 30. Decision: Ten-chunk Worker Throughput and ETA Average
- **Date:** 2026-07-17
- **Status:** Approved
- **Decision:** Retain each worker's last 10 finalized input rates. While active, combine its live assignment rate with up to nine finalized rates; while idle, use finalized history only. Drive `AVG` and worker ETA from this same smoothed rate.
- **Consequences:**
  - Rate and ETA remain responsive without following every callback fluctuation.
  - All worker averages share a bounded 10-chunk horizon.
  - Assignment resets do not erase worker performance history.
