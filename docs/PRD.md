Project-wide Product Requirements Document (PRD) for the new Rust implementation of zipmt.

TLDR:
    Goal: Define the requirements for a high-performance, robust, parallel compression tool in Rust (zipmt-rust).
    Status: PRD drafted based on C and Go implementations.
    Action: Submit to Smith for UX/acceptance review and Morpheus for architectural planning.

# Product Requirements Document (PRD): zipmt-rust

## 1. Executive Summary

`zipmt-rust` is a new, high-performance command-line utility written in Rust that parallelizes data compression across multi-core systems. The project builds upon the lessons of the legacy **C implementation** (high performance but dangerous file-deletion behavior and memory unsafety) and the **Go implementation** (cleaner pipeline abstractions but prone to memory-copy logic bugs and verification panics). 

The core focus of `zipmt-rust` is **execution safety, robustness, memory correctness, and maximum CPU utilization**.

---

## 2. Product Objectives & Target Audience

- **Target Audience:** Systems administrators, developers, DevOps engineers, and automated data pipelines handling large-scale file and stream compression.
- **Key Value Proposition:** Faster compression than single-threaded gzip/bzip2/xz while enforcing type-safe memory boundaries, robust panic recovery, and safe CLI behaviors.

---

## 3. Reference Analysis & Legacy Lessons

The requirements for `zipmt-rust` directly address the structural deficiencies in the legacy codebases:

| Legacy Deficiency | Severity | Action for Rust Implementation |
|-------------------|----------|--------------------------------|
| **C Default File Deletion:** Deletes the source file by default after compression. | **Critical UX Hazard** | Reverted: Keep files by default. Source deletion must be an opt-in CLI flag (`--delete`). |
| **Go Buffer Corruption:** Inverted `copy()` parameters zeroed out buffers. | **Critical Data Loss** | Safe compiler check: Rust's ownership model and unit test integration prevent copy-order mutations. |
| **Go XZ verification panic:** Nil pointer dereference during verification error paths. | **Medium Stability** | Idiomatic Rust `Result<T, E>` and pattern matching on error returns. |
| **C GLib Deprecation:** Obsolete thread/mutex initializations. | **Medium Technical Debt** | Use modern Rust concurrency crates (`rayon`, `crossbeam-channel`, `tokio`). |

---

## 4. Feature Requirements

### FR-1: Multi-Format Compression
- **Requirement:** Support `bzip2`, `gzip`, and `xz` compression formats.
- **Acceptance Criteria:**
  - Compressed files must be fully compliant with standard decompression tools (`bunzip2`, `gunzip`, `unxz`).
  - Automatically detect target format based on output file extension if `--algo` is not specified.

### FR-2: Dual-Mode Concurrency
- **Split Mode (File-Based):** 
  - Partition files statically on disk into `N` chunks (defaults to thread count).
  - Compress chunks in parallel.
  - Assemble parts sequentially without data races.
- **Stream Mode (Pipe-Based):**
  - Read input blocks (default 4MB) from standard input.
  - Pipeline block routing through channels to compression workers.
  - Reorder blocks in memory sequentially before writing to standard output.
  - **Backpressure control:** Limit loaded uncompressed blocks in memory to `N * 2` to prevent Out-Of-Memory (OOM) crashes on large inputs.

### FR-3: Verification Mode (`--test`)
- **Requirement:** Verify the integrity of a compressed file.
- **Acceptance Criteria:**
  - Execute a decompression pass and verify that checksums/header checks succeed.
  - Must return exit code `0` on success, and non-zero on failure.
  - Under no circumstances should verification trigger a panic or crash, even on fully corrupted streams.

### FR-4: Safe Deletion Cycle (`--delete`)
- **Requirement:** Optional file deletion to free up space.
- **Acceptance Criteria:**
  - Source files must **never** be deleted unless `--delete` is explicitly passed and compression has successfully finished (confirmed by checksum/write flushes).
  - If a signal (e.g., SIGINT / Ctrl-C) is received, the tool must immediately abort, delete any partial output, and **preserve the source file**.

---

## 5. Non-Functional & Safety Requirements

- **Memory Correctness:** Compile with zero raw `unsafe` blocks unless wrapping a foreign C compression library.
- **Performance:** Compression speeds must match or exceed the C implementation for similar core counts.
- **Robust Error Handling:** Zero panics at runtime. All IO errors, format mismatches, and lock conditions must be propagated as Rust `Result` types.
- **Exit Statuses:**
  - `0`: Success.
  - `1`: Bad argument or usage error.
  - `2`: IO or compression failure.
  - `3`: Verification failed (corrupted file).

---

## 6. User Interface (CLI Flags)

The CLI parser (recommended: `clap`) must enforce the following flags:

```
zipmt-rust [FLAGS] [OPTIONS] [file]
```

### Flags
- `-h`, `--help`: Prints help information.
- `-V`, `--version`: Prints version information.
- `-t`, `--test`: Runs verification pass on the input file.
- `-d`, `--delete`: Deletes the source file upon successful compression (default: false).
- `-c`, `--stdout`: Writes output to stdout.
- `-v`, `--verbose`: Displays detailed progress bars and threading metrics.

### Options
- `-a`, `--algo <algo>`: Compression algorithm to use (`xz`, `bz2`, `gz`). (Default: `xz`).
- `-j`, `--threads <num>`: Number of worker threads (default: CPU core count).
- `-o`, `--output <path>`: Output destination path.
