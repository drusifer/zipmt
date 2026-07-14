# Task Board: zipmt-rust TUI Testing Sprint

This is the single source of truth for the `zipmt-rust` TUI testing tasks.

## 🎯 Sprint Goal
Decouple the TUI rendering logic and introduce `insta` snapshot tests to visually verify TUI layout outputs.

---

## 📅 Phase Breakdown

### Phase 1: Decoupling and Dependencies [DONE]
- [x] **Task 1.1 (Refactor):** Refactor `draw_tui` to accept any target implementing `std::io::Write`. (Assignee: Neo | UAT: Trin)
- [x] **Task 1.2 (Cargo):** Add `insta` (and `regex` if needed for stripping ANSI escape codes) to dev-dependencies. (Assignee: Neo | UAT: Trin)

### Phase 2: Snapshot Verification [DONE]
- [x] **Task 2.1 (Snapshot Tests):** Write unit-level snapshot tests in `src/tui.rs` verifying Split Mode and Stream Mode renderings. (Assignee: Neo | UAT: Trin)
