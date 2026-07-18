Lessons learned catalog detailing dependency compilation, GLib concurrency deprecation, and safety warnings.

TLDR:
    Impact: Highlights critical setup steps, API warnings, high-risk CLI behaviors, TUI testing strategies, and non-blocking event loop practices.
    Next Steps: Periodically update as build issues or bug discoveries occur.

# Project Lessons Learned (LESSONS.md)

This document indexes critical lessons learned during the development, compilation, and maintenance of `zipmt`.

---

## 1. Lesson: Missing Development Headers for bzip2 Linkage
- **Date:** 2026-07-12
- > **Tags:** #Build #Dependencies #C
- **Context:** Compiling the C codebase on a clean environment fails immediately with `bzlib.h: No such file or directory`.
- **The Issue:** Developers assume that having `bzip2` installed in the OS is sufficient. However, the compiler requires the header files (`bzlib.h`) and static/shared library binaries which are only packaged in the development version of the library.
- **The Solution:** Install `libbz2-dev` (Debian/Ubuntu) or `bzip2-devel` (RHEL/CentOS) before compiling.
- **The Rule:** Always document and install development packages (`-dev` or `-devel`) for all compiled C library dependencies (GLib, bzip2, zlib).

---

## 2. Lesson: GLib Concurrency API Deprecation Warnings
- **Date:** 2026-07-12
- > **Tags:** #API #GLib #Concurrency
- **Context:** The code calls `g_thread_init(NULL)` during initialization.
- **The Issue:** In GLib version 2.32 and newer, the threading system is automatically initialized. The `g_thread_init` function is deprecated, causing compile-time warnings and potential compatibility issues with newer versions of GCC/GLib.
- **The Solution:** Remove the call to `g_thread_init` when compiling against GLib >= 2.32, or handle compilation flags dynamically.
- **The Rule:** Periodically audit legacy thread-initialization code when upgrading platform dependency versions to avoid compiler deprecation warnings.

---

## 3. Lesson: Deletion of Source Material by Default (UX Safety Hazard)
- **Date:** 2026-07-12
- > **Tags:** #Safety #UX #CLI
- **Context:** Test runs of `zipmt` on active project data resulted in the deletion of input files without confirmation.
- **The Issue:** The default mode of operation deletes the source file upon successful compression, which can lead to accidental data loss if the user forgets to supply `-k` or is unaware of this behavior.
- **The Solution:** Prominently place CAUTION and WARNING alerts in all user guides, READMEs, and help outputs.
- **The Rule:** Any utility that performs destructive operations (like deleting original files) must have highly visible warnings in the user interface and documentation.

---

## 4. Lesson: Go `copy()` Parameter Ordering Bug
- **Date:** 2026-07-13
- > **Tags:** #Go #Memory #Bugs
- **Context:** In `zipwriter.go:38`, the code copies uncompressed bytes into a new task block.
- **The Issue:** The Go built-in function is structured as `copy(dst, src)`. The code used `copy(data[start:end], chunk)`, which mistakenly copied from the empty destination `chunk` into the populated source buffer `data`. This resulted in the user's input stream being mutated to all `0`s, and the compressor outputting compressed zeros.
- **The Solution:** Reverse the parameters: `copy(chunk, data[start:start+chunkz])`.
- **The Rule:** ALWAYS verify that Go `copy` calls are written as `copy(destination, source)`. Build active integration tests that verify compressed content matches uncompressed content upon decompression.

---

## 5. Lesson: Defensive Nil-Pointer Checking in Error Handling
- **Date:** 2026-07-13
- > **Tags:** #Go #ErrorHandling #Panic
- **Context:** In `xzzipper.go`, `Verify()` returns `reader.Verify()` only when the reader initialization fails.
- **The Issue:** The statement `if err != nil { err = reader.Verify() }` means that if `xz.NewReader` fails and returns an error, the code attempts to call `.Verify()` on a `nil` `reader` pointer, leading to a nil-pointer dereference panic. If it succeeds, the validation is skipped.
- **The Solution:** Return the error immediately if `err != nil`. Only invoke methods on `reader` when `err == nil`.
- **The Rule:** Never invoke methods on struct pointers returned alongside non-nil errors, as the pointer is likely `nil`.

---

## 6. Lesson: Using `TestBackend` for TUI Layout Snapshot Testing
- **Date:** 2026-07-14
- > **Tags:** #Testing #TUI #Rust #Ratatui
- **Context:** Snapshot testing terminal layouts historically relied on writing raw strings to dummy files, stripping ANSI escape sequences, and asserting them. This was error-prone, hard to format, and broke easily when styling (colors, borders) changed.
- **The Issue:** Traditional integration/PTY testing is heavy, environment-dependent, and hard to run in automated CI pipelines without standard terminals.
- **The Solution:** Use Ratatui's `TestBackend` mock terminal with a fixed size (e.g., 80x15). The test renders the layout onto the `TestBackend` buffer, then converts the buffer cell symbols (`cell.symbol()`) sequentially to a clean, formatted plain-text grid, which is stored and verified using `insta::assert_snapshot!`.
- **The Rule:** Use an in-memory `TestBackend` mock terminal for rendering validation and snapshot assertions. Avoid manual ANSI sequence stripping in favor of structured buffer inspection.

---

## 7. Lesson: Non-blocking Inner Event Draining Loops in Crossterm
- **Date:** 2026-07-14
- > **Tags:** #Crossterm #TUI #EventLoop #Concurrency
- **Context:** The main TUI event loop uses `event::poll` with a tick rate of 100ms.
- **The Issue:** Under heavy event generation (e.g., dragging to resize the terminal window, or holding down speed adjustment/throttling keys), polling once per loop iteration introduces event lag. The queue of terminal inputs becomes backlogged, causing sluggish user responsiveness.
- **The Solution:** Once `event::poll(tick_rate)` returns true, execute a nested non-blocking loop `while event::poll(Duration::from_millis(0)).unwrap_or(false)` to read and process all pending events in the queue in a single frame.
- **The Rule:** When handling input events in crossterm, always wrap the event read in an inner loop polling with zero-duration to drain all immediately pending inputs, preventing command lag.

---

## 8. Lesson: Commitment to Breaking Changes and Structural Consistency
- **Date:** 2026-07-14
- > **Tags:** #Process #Design #Consistency #BreakingChanges
- **Context:** When defaulting the TUI in `zipmt-rust`, the team removed the `-T` flag but kept references to it in integration tests, and missed that stream mode's pipeline logic was disabled by the fallback rules.
- **The Issue:** Partial breaking changes are worse than clean ones. Removing a CLI option but leaving testing scripts and adjacent systems expecting it, or neglecting compatibility in dependent sub-modes, creates structural bugs and confusing UX.
- **The Solution:** If you break it, break it all the way. Align all files, test pipelines, environments, and documentations to the new architecture. Do not leave "dangling" legacy configurations or partial transitions.
- **The Rule:** A breaking change must be executed atomically across the entire codebase—including tests, scripts, and documentations.

---

## 9. Lesson: Dynamic Btop-like Unit Scaling for TUI Graph Ticks
- **Date:** 2026-07-15
- > **Tags:** #TUI #Design #Aesthetics #DynamicScaling
- **Context:** Initial iterations of the speed history graph formatted y-axis values exclusively as MB/s, resulting in confusing labels (e.g. `0.0M`) when speed ranges fell into lower bounds.
- **The Issue:** Displaying static units on y-axis boundaries reduces clarity when data varies across multiple magnitudes (e.g. bytes, kilobytes, megabytes, gigabytes).
- **The Solution:** Implement dynamic, magnitude-based scaling where graph boundaries dynamically format units (e.g. `B` for bytes, `K` for kilobytes, `M` for megabytes, `G` for gigabytes) based on the current peak value.
- **The Rule:** Graphs and charts displaying variable physical units must dynamically adapt labels to match the current order of magnitude of the dataset.

---

## 10. Lesson: In-Memory TUI Log Buffers for Piped Stream Execution
- **Date:** 2026-07-15
- > **Tags:** #TUI #Logging #EventLoop
- **Context:** Adding stdout/stderr logs inside the TUI required intercepting outputs which would otherwise corrupt alternate terminal screens.
- **The Issue:** Standard output or error writes directly inside raw TUI alternate screens cause drawing artifacts and screen garbage.
- **The Solution:** Establish an in-memory, thread-safe global log queue (`OnceLock<Arc<Mutex<Vec<String>>>>`). Intercept internal verbose statements (`log_verbose!`) to populate this buffer when `TUI_ACTIVE` is enabled, and render the buffer as a scrollable paragraph in the layout.
- **The Rule:** Capturing logs in a full-screen TUI requires storing log rows inside thread-safe memory and rendering them via custom scroll-offset bounds.

---

## 11. Lesson: Automated TTY Fallback Detection & Stream Safety
- **Date:** 2026-07-15
- > **Tags:** #CLI #TTY #IsTerminal #Fallback #Redirection
- **Context:** Automatically defaulting to a TUI without checks risks piping raw visual terminal escape codes (e.g. cursor moves, clear screens) into target files or pipelines.
- **The Issue:** Traditional CLI tools require manual flags (like `-T`) to toggles TUIs. If a user pipes output to a file and forgets to disable the TUI, the resulting file is corrupted with ANSI graphics.
- **The Solution:** Automate this check in `main.rs` by checking `std::io::stdout().is_terminal()` and `std::io::stdin().is_terminal()`, alongside flags like `-c`/`--stdout` or stream state. If any indicate redirection, bypass TUI initialization completely.
- **The Rule:** Any CLI application providing a full-screen TUI must automatically fallback to a silent/plain-text output mode when standard streams are piped or redirected, protecting integrity of user data.

---

## 12. Lesson: Unidirectional Channel-Based UI/Engine Decoupling
- **Date:** 2026-07-15
- > **Tags:** #Concurrency #Channel #Architecture #Testing
- **Context:** Decoupling processing from UI rendering using channel progress events.
- **The Issue:** Traditional concurrent programs share a mutable UI state struct across worker threads using arc-mutex wrappers, causing locks and thread contention.
- **The Solution:** Restructure the compression pipeline to communicate with the UI main thread using a unidirectional `mpsc::channel`. The worker threads simply emit lightweight events (`ProgressEvent`) when updates occur. The TUI main thread periodically drains the channel and updates its local state.
- **The Rule:** Unidirectional message passing channels provide a clean boundary for testing and prevent locking contention across worker threads.

---

## 13. Lesson: Mouse Coordinates click tracking in Text Terminals
- **Date:** 2026-07-15
- > **Tags:** #MouseCapture #Crossterm #TUI #Interaction
- **Context:** Supporting mouse click/drag slider controls in a terminal TUI.
- **The Issue:** Text terminals use row/col grid cell systems. Mapping mouse click coordinates requires accounting for centered widgets and offsets.
- **The Solution:** Calculate the absolute centering offset based on terminal size, check if the clicked mouse event falls inside the bounding box of the vertical sliders, and map the row offset directly to the target scale.
- **The Rule:** Capturing mouse interaction in a centered terminal dashboard requires checking if mouse events fall inside dynamic coordinate bounding boxes offset by padding.

---

## 14. Lesson: Emit UI Lifecycle Events Only at State Authority Points
- **Date:** 2026-07-16
- > **Tags:** #Concurrency #Observability #StateMachines #Rust
- **Context:** Showing the same chunk across an input queue, worker slot, pending sorter, and output-ready indicator can easily produce stale or duplicate UI state.
- **The Issue:** Inferring transitions from aggregate counters cannot reliably identify where a particular chunk currently resides.
- **The Solution:** Emit queued events from the reader, assigned/pending events from workers, and written events from the ordered writer, then update all visible stages in one reducer.
- **The Rule:** For concurrent pipeline observability, event producers must be the owners of the transition and the UI must have one projection reducer.

---

## 15. Lesson: Runtime Concurrency Controls Need Future-Work Semantics
- **Date:** 2026-07-16
- > **Tags:** #Concurrency #RuntimeControls #Compression #Safety
- **Context:** Users expect chunk-size and worker-count knobs to react live, but stopping or repartitioning in-flight work risks lost jobs and corrupt ordering.
- **The Issue:** Immediate mutation of active work makes control behavior nondeterministic and complicates safe rollback.
- **The Solution:** Apply chunk sizing when the reader begins the next block and worker gating before a worker accepts its next job. Let all in-flight chunks finish.
- **The Rule:** Live pipeline knobs should affect future work at explicit boundaries unless interruption semantics are intentionally designed and tested.

---

## 16. Lesson: Responsive TUIs Need Shared Render and Input Geometry
- **Date:** 2026-07-16
- > **Tags:** #TUI #ResponsiveDesign #MouseCapture #Testing
- **Context:** Expanding a fixed dashboard to fill larger terminals changes both widget placement and interactive hit zones.
- **The Issue:** Scaling only the renderer leaves mouse controls pointing at obsolete centered coordinates; fixed decorative widths can also truncate variable status labels.
- **The Solution:** Anchor render cards and mouse ranges from the same terminal edges, derive flexible widths from actual strings, and test the minimum size plus at least one larger terminal.
- **The Rule:** Responsive terminal layout and input geometry are one feature and must be derived from the same constraints.

---

## 17. Lesson: Telemetry Sampling Cadence Must Be Independent of Input Events
- **Date:** 2026-07-17
- > **Tags:** #TUI #Telemetry #Sampling #Visualization
- **Context:** A scrolling chart shares an event loop with keyboard, mouse, resize, and progress events.
- **The Issue:** Sampling once per loop iteration makes graph density and reported rates change when the user moves the mouse or presses keys.
- **The Solution:** Gate telemetry on elapsed wall time, normalize counter deltas by actual duration, and store rate plus cumulative values in the same record.
- **The Rule:** Interactive event frequency must never determine telemetry sampling frequency or units.

---

## 18. Lesson: Observability Controls Must Match Their Application Boundary
- **Date:** 2026-07-17
- > **Tags:** #TUI #Concurrency #Affordances #SplitMode
- **Context:** A shared control footer can imply that every setting applies equally to streaming and pre-partitioned parallel work.
- **The Issue:** Split partitions and the Rayon pool are created at startup, and compressor level is loaded when each encoder is created. Leaving those cards interactive creates false feedback without changing active work.
- **The Solution:** Audit where each setting is consumed, keep fixed settings visible for context, label their boundary explicitly, and exclude them from focus, keyboard, and pointer adjustment in modes where they cannot apply.
- **The Rule:** A runtime control is live only when the engine reads it at a documented future-work boundary; otherwise render it as fixed and remove its interaction affordances.

---

## 19. Lesson: Completion Holds Need an Automation Escape Hatch
- **Date:** 2026-07-17
- > **Tags:** #TUI #Automation #Completion #Telemetry
- **Context:** Keeping a successful interactive dashboard open is useful to people but can deadlock forced-TUI integration tests that have no operator to dismiss it.
- **The Solution:** Hold only genuine interactive sessions, retain an explicit automation override that renders completion once and exits, and freeze telemetry so the final moving average does not decay through appended zero samples.
- **The Rule:** Persistent terminal completion states must distinguish interactive ownership from automated rendering and must freeze their final measurement window.

---

## 20. Lesson: Parallel File Slices Do Not Require Parallel Memory Slices
- **Date:** 2026-07-17
- > **Tags:** #Rust #IO #Memory #Parallelism #SplitMode
- **Context:** Parallel compression needs independent byte ranges, but representing those ranges as owned buffers duplicates the entire source and compressed result in memory.
- **The Solution:** Give each worker its own seeked file handle and bounded reader, stream output to temporary storage, and retain only ordered temporary handles for final assembly.
- **The Rule:** For seekable inputs, parallelize by offsets and bounded streams; memory should scale with concurrency and buffer size, never file size.

---

## 21. Lesson: ETA and Graph Rates Serve Different Time Horizons
- **Date:** 2026-07-17
- > **Tags:** #Telemetry #ETA #TUI #Averages
- **Context:** Short-window graph rates communicate current motion but make remaining-time estimates oscillate during buffering, flushes, and worker completion.
- **The Solution:** Keep short-window values for visualization, derive worker summaries from each worker's active lifetime, and derive job ETA from composite work divided by whole-job elapsed time.
- **The Rule:** Never drive ETA directly from the newest visualization sample; select an averaging horizon that matches the prediction being made.

---

## 22. Lesson: Moving-average Windows Need a Stable Bucket Duration
- **Date:** 2026-07-17
- > **Tags:** #Telemetry #Sampling #Graphs #TimeSeries
- **Context:** A moving average expressed only as a sample count changes meaning when sample cadence changes or follows an event loop.
- **The Solution:** Aggregate graph data onto an explicit one-second cadence, normalize deltas by actual bucket duration, and define smoothing as ten consecutive buckets.
- **The Rule:** Time-series smoothing must specify both bucket duration and bucket count so its real time horizon is deterministic.

---

## 23. Lesson: Worker Metrics Must Follow Assignment Lifetimes
- **Date:** 2026-07-17
- > **Tags:** #Workers #Telemetry #StateMachines #StreamMode
- **Context:** Aggregate Stream counters cannot explain why one worker is slow or how far its current chunk has progressed.
- **The Solution:** Reset worker-local bytes and timing at assignment, update them from compressor callbacks, freeze them at completion, and retain the last result after the worker becomes idle.
- **The Rule:** Per-worker rates, ratio, and ETA must share one authoritative assignment lifetime or they will mix unrelated chunks.

---

## 24. Lesson: Dense Worker Telemetry Needs Stable Geometry
- **Date:** 2026-07-17
- > **Tags:** #TUI #Workers #Progress #Layout
- **Context:** Variable-width worker rows make live rates and ETA difficult to compare while values change.
- **The Solution:** Use a bordered card with identity in the title and a one-cell gauge whose label reserves fixed positions for each metric.
- **The Rule:** Frequently changing telemetry should update in place instead of moving neighboring fields.

---

## 25. Lesson: Progress Fill and Statistics Need Separate Rows
- **Date:** 2026-07-17
- > **Tags:** #TUI #Gauge #Readability #Formatting
- **Context:** Labels drawn inside a filled gauge can lose contrast as progress moves beneath them.
- **The Solution:** Keep the gauge on its own row and render fixed-width, fixed-point statistics immediately below it.
- **The Rule:** Persistent measurements should not share their visual layer with changing progress fill.

---

## 26. Lesson: Gauge Label Inversion Requires Both Colors
- **Date:** 2026-07-17
- > **Tags:** #TUI #Contrast #Accessibility #Ratatui
- **Context:** A gauge foreground without an explicit background leaves its filled label dependent on terminal defaults.
- **The Solution:** Define both sides of the contrast pair and test the rendered cell colors after the fill crosses the centered label.
- **The Rule:** Never rely on terminal-default colors for text rendered over dynamic fill.

---

## 27. Lesson: Signed Series Avoid Stacked-chart Drift
- **Date:** 2026-07-17
- > **Tags:** #TUI #Charts #Ratatui #Telemetry
- **Context:** Mirrored input/output can be rendered as two charts, but independent axes and plot areas can drift.
- **The Solution:** Put input above zero and output below zero in one native multi-series chart with shared bounds.
- **The Rule:** When related series share magnitude semantics, prefer signed values on one scale over separately scaled panels.

---

## 28. Lesson: Streaming Encoder Output Is Not a Stable Ratio Denominator
- **Date:** 2026-07-17
- > **Tags:** #Compression #Telemetry #Workers #UX
- **Context:** Encoders may buffer almost all output while reporting only a small header during progress callbacks.
- **The Solution:** Keep byte progress live but calculate displayed compression ratio only from finalized encoder output.
- **The Rule:** Do not project a compression ratio from an output counter whose buffering semantics are unknown.

---

## 29. Lesson: Worker Trends Should Outlive One Assignment
- **Date:** 2026-07-17
- > **Tags:** #Workers #MovingAverage #Compression #Telemetry
- **Context:** A final-only ratio is truthful but disappears while a worker starts its next chunk.
- **The Solution:** Retain a bounded history of finalized assignment ratios per worker and display their moving average during future work.
- **The Rule:** Operational trend metrics should span completed work units while excluding incomplete measurements.

---

## 30. Lesson: Smoothed ETA and Displayed Rate Must Share a Source
- **Date:** 2026-07-17
- > **Tags:** #Workers #ETA #MovingAverage #Telemetry
- **Context:** Showing a smoothed throughput while deriving ETA from a different instantaneous rate makes the panel internally inconsistent.
- **The Solution:** Use one bounded worker-rate average for both the displayed AVG and remaining-time calculation.
- **The Rule:** A projected ETA should be derived from the exact rate presented beside it.
