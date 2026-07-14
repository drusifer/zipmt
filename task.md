# Task Board: zipmt-rust TUI UX Upgrade Sprint

This is the single source of truth for the `zipmt-rust` TUI UX Upgrade tasks.

## 🎯 Sprint Goal
Upgrade the TUI UX with retro Star Trek LCARS diagnostics styling, real-time ETA calculation for split mode, and capacity projections for stream mode.

---

## 📅 Phase Breakdown

### Phase 1: State & ETA Calculation [DONE]
- [x] **Task 1.1 (State Size):** Capture total input size for Split Mode and pass it to `TuiState`. (Assignee: Neo | UAT: Trin)
- [x] **Task 1.2 (ETA Formula):** Implement running ETA countdown logic based on average bytes-per-second processed. (Assignee: Neo | UAT: Trin)

### Phase 2: Stream Forecasts [DONE]
- [x] **Task 2.1 (Forecast Math):** Implement time-capacity projection forecasting (1m, 5m, 10m) inside Stream Mode. (Assignee: Neo | UAT: Trin)

### Phase 3: Retro LCARS Visuals [DONE]
- [x] **Task 3.1 (LCARS Layout):** Re-skin TUI outputs with colorful LCARS panel bars, custom bracket panels, and retro styling. (Assignee: Neo | UAT: Trin)
- [x] **Task 3.2 (Snapshot Sync):** Run snapshot verification tests and update snap reference definitions. (Assignee: Neo | UAT: Trin)
