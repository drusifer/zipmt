# Task Board: zipmt-rust TUI Ratatui Migration Sprint

This is the single source of truth for the `zipmt-rust` TUI Ratatui Migration tasks.

## 🎯 Sprint Goal
Migrate the TUI from custom printed text formatting to the widget-based Ratatui library, integrating keyboard event polling directly into the main-thread event loop, and ensuring all snapshot/integration tests pass cleanly.

---

## 📅 Phase Breakdown

### Phase 1: Dependencies & TUI Setup
- [x] **Task 1.1 (Cargo):** Add `ratatui` dependency to `Cargo.toml`. (Assignee: Neo | UAT: Trin)
- [x] **Task 1.2 (CLI & Fallbacks):** Remove `-T` / `--tui` CLI flag from `main.rs` and make TUI the default. Implement auto-redirection / fallback checks (disable TUI mode when output is piped or standard streams are not interactive TTYs). (Assignee: Neo | UAT: Trin)
- [x] **Task 1.3 (Terminal Init):** Set up initialization of `stderr` alternate screen raw mode via Ratatui terminal. (Assignee: Neo | UAT: Trin)

### Phase 2: Event Loop & Throttling
- [x] **Task 2.1 (Main Event Loop):** Replace the background keyboard listener thread with crossterm main event loop polling at a tick rate of 100ms or 250ms. (Assignee: Neo | UAT: Trin)
- [x] **Task 2.2 (Keyboard Handlers):** Implement keystroke event handlers inside the main event loop (`+`/`=`, `-`, `p`/`P`, `q`/`Esc`) updating atomic speed and pause variables, and handling immediate safe abort. (Assignee: Neo | UAT: Trin)
- [x] **Task 2.3 (Worker Sync):** Coordinate thread safety between the background worker compression thread and the main loop. (Assignee: Neo | UAT: Trin)

### Phase 3: Widgets & Layout Snapshots
- [x] **Task 3.1 (Ratatui Widgets):** Re-render LCARS widgets (System status, Split mode stripes, Stream mode capacity gauge, speed graph via canvas/columns, and control dashboard). (Assignee: Neo | UAT: Trin)
- [x] **Task 3.2 (Snapshot Migration):** Migrate layout snapshot tests in `tui.rs` to use Ratatui `TestBackend`, ensuring full formatting and alignment verification. (Assignee: Neo | UAT: Trin)
- [x] **Task 3.3 (UAT Sync):** Verify all unit, integration, and snapshot tests pass cleanly under `make test-rust`. (Assignee: Neo | UAT: Trin)
