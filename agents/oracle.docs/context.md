# Agent Local Context (context.md)

This file tracks the accumulated knowledge, project structure, and documentation state maintained by the Knowledge Officer (Oracle).

## Recent Decisions
- **Groomed graph/worker observability (2026-07-17)**: Recorded Decisions 21-22 and Lessons 22-23 for deterministic time buckets and assignment-scoped worker metrics.
- **Groomed slice observability (2026-07-17)**: Recorded Decision 20 for slice/composite/process telemetry and Lesson 21 separating ETA and graph-rate time horizons.
- **Groomed bounded Split streaming (2026-07-17)**: Recorded Decision 19 for seeked ranges/temp sections and Lesson 20 that parallel file slices need not become owned memory slices.
- **Groomed completion/smoothed I/O increment (2026-07-17)**: Recorded Decision 18 for persistent final dashboards and MA5 RATE projection, plus Lesson 19 for automation escape and frozen telemetry.
- **Groomed Split TUI uplift (2026-07-17)**: Recorded Decision 17 for authoritative Split lifecycle/aggregate projection and truthful fixed controls; recorded Lesson 18 requiring affordances to match the engine's application boundary.
- **Groomed mirrored I/O chart sprint (2026-07-17)**: Recorded Decision 16 for fixed-cadence dual-mode mirrored history and Lesson 17 separating telemetry cadence from interactive event frequency.
- **Groomed Pipeline Flow sprint (2026-07-16)**: Added Decisions 13-15 for authoritative lifecycle projection, future-only runtime controls, and responsive full-canvas TUI geometry. Added Lessons 14-16 covering event authority, safe future-work semantics, and shared render/input geometry.
- **Groomed Sprint Documentation**: Updated the Architectural Decision Record ([DECISIONS.md](file:///home/drusifer/Projects/zipmt/DECISIONS.md)) to capture the migration of the TUI to the Ratatui widget-based library (Decision 7) and the 100ms crossterm event polling loop with non-blocking event draining (Decision 8).
- **Groomed TUI Defaulting & Fallbacks**: Updated DECISIONS.md with Decision 9 and LESSONS.md with Lesson 11 to document the TUI defaulting and TTY fallback implementation.
- **Groomed Decoupling & Interactive LCARS UI documentation**: Captured Decisions 10-12 in `DECISIONS.md` and Lessons 12-13 in `LESSONS.md` to catalog the modular pipeline, dynamic controller, slider widgets, Tab focus, and mouse click captures. Verified task board is completely updated in `task.md`.
- **Captured Core Lessons**: Updated the Lessons Learned catalog ([LESSONS.md](file:///home/drusifer/Projects/zipmt/LESSONS.md)) to document best practices for using `TestBackend` mock terminals for layout snapshot tests (Lesson 6) and non-blocking zero-duration crossterm event polling to prevent queue lag (Lesson 7).
- **Updated Mindmap**: Updated the Repository Mindmap ([MINDMAP.md](file:///home/drusifer/Projects/zipmt/MINDMAP.md)) with the complete directory tree map and logic flow table for `zipmt-rust` to finalize developer resource grooming.
- **Verified Task Board**: Confirmed that all TUI Ratatui Migration tasks are marked as completed in the root [task.md](file:///home/drusifer/Projects/zipmt/task.md) task board.

---
*Last updated: 2026-07-17T15:22:00-04:00*
