# Task Board: zipmt-rust TUI Sprint

This is the single source of truth for the `zipmt-rust` TUI progress display tasks.

## 🎯 Sprint Goal
Add a TUI progress display option (`--tui` / `-T`) to visually display queue depths, concurrency states, file split stripes, and I/O compression throughput rates in real-time.

---

## 📅 Phase Breakdown

### Phase 1: CLI and Shared State [DONE]
- [x] **Task 1.1 (CLI):** Integrate `--tui` / `-T` flag into `clap` Args and ensure logs are suppressed in TUI mode. (Assignee: Neo | UAT: Trin)
- [x] **Task 1.2 (State):** Build the thread-safe shared `TuiState` structure and initialize it dynamically based on selected run mode. (Assignee: Neo | UAT: Trin)

### Phase 2: Split Mode Rendering [DONE]
- [x] **Task 2.1 (Stripe Progress):** Add callbacks/atomic counters inside the Rayon loop to increment processed/written bytes per split. (Assignee: Neo | UAT: Trin)
- [x] **Task 2.2 (Draw Split):** Build the ANSI rendering layout for Split Mode displaying stripe progress bars, speeds, and compression ratios. (Assignee: Neo | UAT: Trin)

### Phase 3: Stream Mode & Integration [DONE]
- [x] **Task 3.1 (Stream Progress):** Implement throughput (MB/s) and queue depth indicators inside the Reader/Writer channels loop. (Assignee: Neo | UAT: Trin)
- [x] **Task 3.2 (Draw Stream):** Build the ANSI rendering layout for Stream Mode displaying queue utilization and current throughput. (Assignee: Neo | UAT: Trin)
- [x] **Task 3.3 (Integration UAT):** Write tests verifying TUI mode starts, runs without panics, and safely redirects display to stderr. (Assignee: Neo | UAT: Trin)
