# Agent Local Context (context.md)

This file tracks the HCI evaluations, gate reviews, and usability criteria maintained by the HCI Expert (Smith).

## Recent Decisions
- **Judge tool/skill TES (2026-07-18)**: Scored the complete active Codex rollout from its July 16 start at 82/100. Nine raw flags reduce to one confirmed Via bypass. Active/latest discovery, Codex skill-load visibility, and shell-rule false positives require correction.
- **Approved completion/smoothed I/O real UX (2026-07-17)**: Actual 80x22/120x30 sessions retain COMPLETE with final stats and close guidance; RATE exposes its MA5 overlay and stable labels.
- **Approved real Split TUI uplift UX (2026-07-17)**: Actual 80x22 and 120x30 Split runs show readable aggregate/lifecycle state, explicit range/overflow, mirrored chart parity, larger-terminal expansion, and honest fixed Level/Partition/Pool presentation.
- **Approved Split TUI uplift architecture (2026-07-17)**: Typed sector stages, completed-only ratio, rate-gated ETA, responsive range/overflow paging, and shared mirrored chart provide truthful visibility. Split Level/Partition/Pool remain visible but fixed and absent from focus/input; Pause/Throttle remain live.
- **Approved Split TUI uplift stories (2026-07-17)**: Aggregate status, one-based WAIT/RUN/DONE sectors, mirrored I/O parity, explicit overflow/paging, and visibly fixed Partition/Pool controls address current status visibility and false-affordance problems. Fixed controls must remain in visual order but be skipped by focus/input.
- **Approved real I/O chart UX (2026-07-17)**: Actual 80x22 and 120x30 PTY streams show smooth right-to-left history motion, clearly mirrored input/output, honest `/s` rate units, live `I` switching to CUMULATIVE and back without reset, preserved lifecycle labels, and expanded readable gauges.
- **Approved combined I/O chart plan (2026-07-16)**: Mirrored input/output orientation, a visible `I` mode toggle, shared scaling, fixed-cadence history, and height-driven knobs satisfy system visibility and user control. Required RATE values to normalize deltas to bytes per second and show `/s`, while CUMULATIVE displays session totals.
- **Approved responsive Phase 3 UX (2026-07-16)**: The corrected 80-column header shows the complete RUNNING state, while terminals above 80x22 use their full canvas. Larger widths improve stage separation, larger heights expose more logs, and the four controls remain consistently right-anchored with matching mouse geometry.
- **Phase 3 real PTY test (2026-07-16)**: Flow stages and four controls match the approved mental model and are readable without relying on color. The discovered header truncation defect was corrected and regression-locked.
- **Approved Pipeline Flow architecture (2026-07-16)**: Typed stage events, fixed-pool worker gating, future-read chunk sizing, and four-card controls preserve visibility and user control. Any control not applicable to an in-flight split partition must be visibly labeled rather than appearing live.
- **Approved Pipeline Flow stories with guardrails (2026-07-16)**: Queue stages must read Input → Workers → Pending Sort → Output Ready, display stable one-based `#N` identities, use textual states rather than color alone, expose all four knobs in visual focus order, and collapse overflow explicitly on small terminals.
- **Approved Interactive LCARS Sliders & Decoupled Usability (2026-07-15)**: Usability testing completed. Verified interactive sliders navigation (Tab focus) and keyboard/mouse adjustment. Confirming that focus visual highlights (different border colors for selected widgets) and sliders rendering (vertical bars) provide excellent system feedback (Heuristic 1) and user control (Heuristic 3).
- **Gate 1 & 2 Combined Approval for Interactive TUI & Decoupled Pipeline (2026-07-15)**: Reviewed and approved stories and architecture in `docs/USER_STORIES_RATATUI_UPGRADE.md`, confirming compliance of modular pipeline, `PipelineController`, vertical sliders, and snapshot test mocks with HCI principles.
- **Gate 1 & 2 Combined Approval for TUI Defaulting & Fallbacks (2026-07-15)**: Reviewed and approved Story 3 and Section 3 of `docs/USER_STORIES_RATATUI.md` under HCI principles (consistency, error prevention), ensuring robust defaulting and redirection checks to prevent data corruption.
- **Approved TUI Defaulting & Fallbacks Usability (2026-07-15)**: Tested built release binary. Verified that redirection correctly falls back to non-TUI mode, and no terminal escapes leak to stdout.
- **Approved Phase 3 Ratatui TUI Rendering (2026-07-14)**: Usability testing (`*user test`) completed and approved. Verified migration of TUI layouts to Ratatui widgets, ensuring proper integration of retro LCARS palette colors and borders, system metrics visibility, and robust layout tests.
- **Approved Ratatui Stories & Architecture**: Approved the combined specifications in [docs/USER_STORIES_RATATUI.md](file:///home/drusifer/Projects/zipmt/docs/USER_STORIES_RATATUI.md) for UX and implementation safety.
- **Approved Phase 2 Keyboard Controls & Throttling (2026-07-14)**: Verified correct behavior of keyboard event loop, real-time LCARS status rendering, pause/resume mechanisms, +/- throttling controls, and clean abort signal handling.

## Key Findings
- **LCARS Retro Theme Guidelines**: Ratatui colors successfully mapped (Orange 208 for borders/panels, Cyan 117 for metrics/data, Lavender 147 for header labels, and Yellow 220 for the rolling history chart).
- **Auto TUI Fallback**: Disabling the TUI automatically during output pipe/redirection prevents ANSI sequence clutter from corrupting compressed files, resolving a critical CLI usability hazard (Heuristic 5: Error Prevention).
- **Throttling and Pause Responsive UX**: Keyboard throttling (+/- speed controls) and pause (P) react instantly at a 64KB chunk resolution. 
- **Graceful Abort & Cleanup**: Hitting Q/Esc cleanly exits the application with exit code 2 and removes the incomplete/corrupt destination file, preventing disk litter (Heuristic 5: Error Prevention, Heuristic 3: User Control & Freedom).
- **Speed History Preservation**: During Pause (P), speed history updates are frozen rather than zeroed out. This maintains historical context for the user when resuming (Heuristic 1: Visibility of System Status).
- **Heuristic 1 (Visibility of System Status)**: Real-time cumulative statistics, Sector-level progress bar gauges (Split Mode), Transporter buffer depth/capacity (Stream Mode), speed projections, and 35-second rolling speed charts provide continuous and rich status feedback.
- **Heuristic 8 (Aesthetic & Minimalist Design)**: Centered layout logic (constrained to 80x15 viewport) looks stunning on any terminal size and avoids visual noise, organizing blocks via structured boundaries and unified labels.

---
*Last updated: 2026-07-18T18:34:00-04:00*
