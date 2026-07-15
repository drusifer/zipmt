# Agent Local Context (context.md)

This file tracks the current state of code implementations and tech stacks maintained by the Software Engineer (Neo).

## Recent Decisions
- **Implemented LCARS Dashboard**: Added full-screen grids and rolling history sparkline graph.
- **Implemented Dynamic Throttling**: Added keyboard control event listeners (`+`/`-`/`p`/`q`).
- **Fixed Stream Crash & Added Centering**: Prevented `usize` subtraction underflows inside the progress bar rendering loops and added horizontal/vertical screen padding using crossterm size queries.
- **Standardized Border Alignment**: Formatted all dashboard panels to exactly 80 characters wide for straight vertical borders.
- **Implemented Safe Sizing & Resize Event Tracking**: Added fallback environment checks, command queries (`tput cols`/`lines`), and crossterm resize listeners to correctly center output even when stdout is redirected (e.g. `> /dev/null`).
- **Added Redirection Sizing Fixes & TDD Coverage**: Used `/dev/tty` and `/dev/stderr` standard descriptor streams to query terminal boundaries via `stty size` under redirected pipe conditions.
- **Moved TUI Loop to Main Thread**: Spawns compression process on a background thread and runs Crossterm event/raw mode logic on the main thread for optimal signal/stream control.
- **Aligned Section Headers**: Audited and padded Split Mode and Stream Mode header dividers to perfectly align at 80 characters.
- **Aligned Progress Bars**: Padded Split Mode progress bar layout from 39 characters down to exactly 38 characters for perfect alignment of the middle vertical divider `│`.
- **Phase 1 of Ratatui Migration**: Completed Task 1.2 and Task 1.3. Removed the `-T`/`--tui` command line flag from main, defaulting to TUI mode with fallback checks for non-TTY or stdout redirection. Initialized the `stderr` alternate screen raw mode using the `ratatui::Terminal` inside `TerminalGuard`.
- **Phase 2 of Ratatui Migration**: Completed Task 2.1, 2.2, and 2.3. Replaced the non-blocking polling and manual thread sleep loop with main-thread event loop polling (`crossterm::event::poll(tick_rate)`) at a tick rate of 100ms, draining all events per iteration to maintain responsiveness and keep the worker compression thread and user inputs properly synchronized.
- **Phase 3 of Ratatui Migration**: Completed Task 3.1, 3.2, and 3.3. Re-rendered all LCARS dashboard layout panels and components (System Status, Sectors progress list, Transporter buffer capacity, rolling speed history graph, and controls panel) using Ratatui widgets (`Paragraph`, `Line`, `Span`, `Style`) and `Layout` constraints centered dynamically on the screen. Migrated layout snapshot tests to use Ratatui's `TestBackend` buffer cell symbol assertions instead of raw string ANSI stripping, ensuring clean, pixel-perfect formatting validation.

---
*Last updated: 2026-07-14T20:06:50-04:00*
