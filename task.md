# Task Board: zipmt-rust Decoupling & Interactive TUI Upgrade Sprint

This is the single source of truth for the `zipmt-rust` Decoupling & Interactive TUI Upgrade tasks.

## 🎯 Sprint Goal
Decouple the frontend interface from the compression pipeline using a channel-based progress event stream, implement a thread-safe library API controller for dynamic configuration adjustments, restore the `-T` flag with fallback safety, build an interactive LCARS dashboard with focusable vertical sliders (supporting keyboard navigation and mouse click/drag), and refactor tests to use mock snapshot rendering.

---

## 📅 Phase Breakdown

### Phase 1: Modular Pipeline & Decoupling (R1/R2)
- [x] **Task 1.1 (Pipeline Abstraction):** Refactor compression pipeline to run independently and expose progress/metric streams. (Assignee: Neo | UAT: Trin)
- [x] **Task 1.2 (Pipeline Controller):** Implement thread-safe `PipelineController` to dynamically alter parameters (level, delay) and handle pause/resume/abort. (Assignee: Neo | UAT: Trin)
- [x] **Task 1.3 (CLI & Flag Restoration):** Restore `-T` / `--tui` CLI flag in main.rs, fallback to standard stream mode if not TTY or redirected. (Assignee: Neo | UAT: Trin)

### Phase 2: Interactive Controls & Vertical Sliders (R4)
- [x] **Task 2.1 (Vertical Sliders):** Render knobs (Throttle delay and Compression level) in vertical slider columns showing level values. (Assignee: Neo | UAT: Trin)
- [x] **Task 2.2 (Keyboard Navigation):** Add Tab cycling with focus highlight and Up/Down/+/ - keys for active knob adjustments. (Assignee: Neo | UAT: Trin)
- [x] **Task 2.3 (Mouse Interaction):** Track mouse click/drag events via Crossterm to adjust slider settings. (Assignee: Neo | UAT: Trin)

### Phase 3: Verification & Snapshots (R5)
- [x] **Task 3.1 (Decoupled Snapshot Tests):** Refactor tests to render TUI layouts using mock metrics on TestBackend. (Assignee: Neo | UAT: Trin)
- [x] **Task 3.2 (UAT and Compilation Checks):** Verify compilation and pass `make test-rust` cleanly. (Assignee: Neo | UAT: Trin)
