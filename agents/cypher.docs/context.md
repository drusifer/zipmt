# Agent Local Context (context.md)

This file tracks the current state of product stories, features, and roadmaps maintained by the Product Manager (Cypher).

## Recent Decisions
- **Split-mode TUI uplift stories (2026-07-17)**: Uplift centers on aggregate progress/ETA, one-based sector lifecycle rows, mirrored rate/cumulative I/O history, responsive sector paging, and truthful startup-fixed Partition/Pool controls without changing Split compression semantics.
- **Launched mirrored I/O chart increment (2026-07-17)**: Stream mode now shows smooth shared-scale input/output history with RATE and CUMULATIVE views, preserves the lifecycle panel, and expands chart plus four knob gauges on taller terminals.
- **I/O chart follow-on requirements (2026-07-16)**: Stream mode needs a mirrored scrolling bytes-in/top and bytes-out/bottom chart with a visible `I` toggle between rate deltas and cumulative totals. It must preserve the 80x22 contract and lifecycle flow while using taller terminals for both more chart space and taller four-knob gauges.
- **Launched Pipeline Flow Observability increment (2026-07-16)**: Shipped one-based chunk tracking across input/workers/pending/output-ready, live chunk-size and worker controls, level-9 defaults, piped-input TUI support, and responsive full-canvas behavior above 80x22.
- **Pipeline Flow Observability sprint (2026-07-16)**: Defined per-slot chunk-number visibility across input, worker, pending/sort, and output-ready stages; runtime chunk-size values from 64 KiB to 8 MiB with a 1 MiB default; active workers from 1 to configured capacity; and compression level 9 defaults in all modes.
- **Drafted Ratatui stories and architecture**: Combined product stories and Morpheus's architecture into [docs/USER_STORIES_RATATUI.md](file:///home/drusifer/Projects/zipmt/docs/USER_STORIES_RATATUI.md).
- **Drafted TUI UX stories**: Created `docs/USER_STORIES_TUI_UX.md`.
- **Drafted LCARS Full-Screen stories**: Created `docs/USER_STORIES_LCARS.md`.
- **Refined CLI TUI Defaulting & Fallback Stories**: Updated User Story 3 in `docs/USER_STORIES_RATATUI.md` to define default TUI operation, removal of CLI flags, and fallback TTY logic (R3).
- **Drafted LCARS Upgrade Stories**: Created `docs/USER_STORIES_RATATUI_UPGRADE.md` containing stories and acceptance criteria for R1 (Front-end abstraction), R2 (Modular library API), R3 (CLI and -T flag restoration), R4 (Star Trek LCARS vertical sliders, keyboard Tab/Up/Down adjustments, mouse integration), and R5 (Decoupled layout snapshot tests).

---
*Last updated: 2026-07-17T14:51:00-04:00*
