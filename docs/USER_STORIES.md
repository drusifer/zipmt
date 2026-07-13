User stories and acceptance criteria for zipmt-rust development.

TLDR:
    Goal: Define the user stories and acceptance criteria (AC) for zipmt-rust.
    Status: Stories defined for core features, pipeline streaming, safety, and testing.
    Action: Defer to Smith for Gate 1 UX review.

# User Stories: zipmt-rust

This document lists the user stories and acceptance criteria (AC) for the new Rust implementation.

---

## Story 1: Parallel Multi-Format Compression
**As a** system administrator,
**I want to** compress large files using multiple CPU cores in `bzip2`, `gzip`, or `xz` format,
**So that** I can minimize the time spent waiting for backups and log archiving.

### Acceptance Criteria
- **AC 1.1:** The utility compiles and runs on UNIX and Windows systems.
- **AC 1.2:** The user can specify the algorithm using `-a` or `--algo` with values `bz2`, `gz`, or `xz`.
- **AC 1.3:** Output files are fully compatible with system-default utilities (`bunzip2`, `gunzip`, `unxz`).
- **AC 1.4:** The user can override worker thread count using `-j` or `--threads`. If omitted, it automatically scales to the system CPU core count.
- **AC 1.5:** Specifying an invalid algorithm name prints an error and returns exit code `1` (bad arguments).

---

## Story 2: Safe File Lifecycle
**As a** developer running automated cleanups,
**I want** my original files preserved by default, with an optional flag to delete them,
**So that** I do not accidentally lose data due to utility deletion defaults or interrupted runs.

### Acceptance Criteria
- **AC 2.1:** By default, the source input file is preserved upon successful compression.
- **AC 2.2:** The source input file is deleted **only** if the `--delete` or `-d` flag is explicitly provided and the compression successfully finishes (status code `0`).
- **AC 2.3:** If the program receives an interrupt signal (SIGINT/Ctrl-C) or encounters a write error, it must cleanly exit, delete the partial/corrupted output file, and **preserve the original input file** (even if `--delete` was passed).

---

## Story 3: Stream Pipeline Concurrency
**As a** data pipeline architect,
**I want to** pipe large streams of data through `zipmt-rust` using standard input and output,
**So that** I can compress intermediate data on the fly without writing huge uncompressed files to disk.

### Acceptance Criteria
- **AC 3.1:** The program supports piping via stdin using `"-"` or omitting input arguments (e.g., `cat data | zipmt-rust -c > data.xz`).
- **AC 3.2:** Data is processed in-memory in sequential blocks (default 4MB).
- **AC 3.3:** The pipeline must throttle reading if worker threads are fully loaded. Peak memory usage for active blocks must not exceed `threads * 2 * 4MB`.
- **AC 3.4:** Blocks must be written to the output stream in the exact sequential order in which they were read.

---

## Story 4: Stream/File Integrity Verification
**As a** QA engineer,
**I want to** run a verification check on compressed files,
**So that** I can guarantee the integrity of my backups without manually decompressing them to disk.

### Acceptance Criteria
- **AC 4.1:** The program executes in test-only mode when `--test` or `-t` is provided.
- **AC 4.2:** The test mode runs a complete decompression verification loop in-memory.
- **AC 4.3:** Verification must return exit code `0` on success and exit code `3` if the file is corrupted.
- **AC 4.4:** The verification loop must handle truncated files, wrong formats, or garbage inputs safely, printing a description to stderr without triggering a thread panic or crash.
