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
