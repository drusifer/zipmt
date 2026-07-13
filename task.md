# Task Board: zipmt-rust Implementation Sprint

This is the single source of truth for the `zipmt-rust` development tasks.

## 🎯 Sprint Goal
Build a high-performance, robust parallel compression utility in Rust supporting bzip2, gzip, and xz, with safe file deletion lifecycle defaults and zero-panic verification loops.

---

## 📅 Phase Breakdown

### Phase 1: Project Setup & Core Engines [DONE]
- [x] **Task 1.1 (Setup):** Initialize Cargo binary project layout, configure CLI dependencies in `Cargo.toml`, and enforce `#![deny(unsafe_code)]` at the crate root. (Assignee: Neo | UAT: Trin)
- [x] **Task 1.2 (Engines):** Implement polymorphic `Compressor` trait and its concrete backend adapters for `bzip2`, `flate2` (gzip), and `xz2`. (Assignee: Neo | UAT: Trin)
- [x] **Task 1.3 (Tests):** Write unit tests in `src/compressor.rs` validating compression and decompression loops on varying byte buffers. (Assignee: Neo | UAT: Trin)

### Phase 2: Dual Concurrency Pipelines [DONE]
- [x] **Task 2.1 (Split Mode):** Implement static file-chunk partitioning and data-parallel compression utilizing Rayon's thread pool. (Assignee: Neo | UAT: Trin)
- [x] **Task 2.2 (Stream Mode):** Build channel-based streaming pipelines (Reader thread $\rightarrow$ Job channel $\rightarrow$ Compression workers $\rightarrow$ Result channel $\rightarrow$ Writer thread). (Assignee: Neo | UAT: Trin)
- [x] **Task 2.3 (Ordering & Flow):** Implement in-memory BTreeMap reordering cache and bounded channel backpressure to cap memory consumption. (Assignee: Neo | UAT: Trin)

### Phase 3: CLI, Safety & Integration UAT [DONE]
- [x] **Task 3.1 (CLI & Signal):** Integrate `clap` CLI parser, implement Ctrl-C signal handlers, and establish standard file-preservation defaults. (Assignee: Neo | UAT: Trin)
- [x] **Task 3.2 (Verification):** Build the `--test` decompression verification mode ensuring graceful error handling without panics. (Assignee: Neo | UAT: Trin)
- [x] **Task 3.3 (Integration):** Add comprehensive integration tests running the built binary against sample files and standard input streams. (Assignee: Neo | UAT: Trin)
