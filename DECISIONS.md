Architectural Decision Record (ADR) detailing design choices for zipmt.

TLDR:
    Impact: Captures structural decisions (GLib, file-deletion by default, static splitting, stream throttling).
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
