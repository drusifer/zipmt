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


