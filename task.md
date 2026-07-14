# Task Board: zipmt-rust TUI LCARS Sprint

This is the single source of truth for the `zipmt-rust` TUI LCARS Upgrade tasks.

## 🎯 Sprint Goal
Upgrade TUI to a full-screen alternate-screen console in Star Trek LCARS style, with live historical speed charts, pretty progress bars, and dynamic keyboard-based speed throttling/pausing.

---

## 📅 Phase Breakdown

### Phase 1: Throttling Controls & Cargo Dependencies [DONE]
- [x] **Task 1.1 (Cargo):** Add `crossterm` dependency to `Cargo.toml`. (Assignee: Neo | UAT: Trin)
- [x] **Task 1.2 (Throttling Interceptor):** Implement `THROTTLE_DELAY_MS` and `IS_PAUSED` atomic variables, and inject sleep loops inside `compress_with_progress` block writes. (Assignee: Neo | UAT: Trin)

### Phase 2: Screen Buffer & Keyboard Event Loop [DONE]
- [x] **Task 2.1 (Alternate Screen):** Set up Crossterm screen handlers to enter raw alternate screen on startup and restore terminal state on exit/signal cleanup. (Assignee: Neo | UAT: Trin)
- [x] **Task 2.2 (Keyboard Thread):** Spawn a keyboard listener thread polling for `+`/`=` (Speed Up), `-` (Slow Down), `p`/`P` (Pause), and `q`/`Esc` (Quit/Abort). (Assignee: Neo | UAT: Trin)

### Phase 3: LCARS UI Rendering & Verification [DONE]
- [x] **Task 3.1 (LCARS Dashboard):** Build the box-drawing grid layouts (System, Sectors, History, Controls) using solid blocks (`█` / `░`) for progress indicators. (Assignee: Neo | UAT: Trin)
- [x] **Task 3.2 (History Chart):** Implement the MB/s rolling speed buffer and render a vertical bar graph using block-height characters. (Assignee: Neo | UAT: Trin)
- [x] **Task 3.3 (UAT Sync):** Run verification tests, update layout snapshots, and ensure clean stderr outputs. (Assignee: Neo | UAT: Trin)
