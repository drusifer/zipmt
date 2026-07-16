# Original User Request

## 2026-07-15T20:20:03Z

Migrate the zipmt-rust TUI from custom text formatting to the widget-based Ratatui library, maintaining the retro LCARS style, rolling speed charts, progress bars, and dynamic keyboard-based throttling/pausing controls.

Working directory: /home/drusifer/Projects/zipmt
Integrity mode: development

## Requirements

### R1. Ratatui TUI Migration
Migrate the existing custom console TUI in `zipmt-rust` to a full Ratatui application. The interface must use Ratatui widgets (or custom rendering onto a Ratatui `Frame`) and Crossterm backend. It must retain the retro Star Trek LCARS style (colors, box borders), showing the progress of stripes/splits in Split mode (bytes read/written, progress bars side-by-side for each split), bytes read/written and queue depth in Stream mode, rolling MB/s speed history rendered as a vertical bar chart, and controls dashboard.

### R2. Main Thread Event Polling Loop
Replace the background keyboard event listener thread with a standard Ratatui main event loop. The loop must poll Crossterm events at a reasonable tick rate (e.g. 100ms or 250ms). It must listen for key inputs:
- `+` or `=` to increase speed (decrease delay)
- `-` to decrease speed (increase delay)
- `p` or `P` to pause/resume toggle
- `q` or `Esc` to quit/abort compression

### R3. Command Line & Output Defaulting
The TUI must be the default/only interface for normal file and stream compression operations without requiring a `-T` or `--tui` flag. The CLI should not expose a `-T` flag. The TUI mode must be automatically disabled if standard output is redirected/piped (e.g., `-c` flag or streaming output to standard output) or if either stdin/stdout is not a TTY, falling back to a clean exit/standard stream logic so as not to corrupt binary output.

### R4. Test Suite Compatibility & Snapshots
Keep the `make test-rust` test suite passing. Update the existing snapshot testing structure to use Ratatui's `TestBackend` or another mock buffer, ensuring the snapshots continue to correctly verify the alignment, formatting, and content of the TUI layout.

### R5. Bob Protocol Compliance
The agent team must strictly adhere to the Bob Protocol persona workflows as defined in `AGENTS.md` (State Management Protocol, Persona Handoffs, and updating the state files under the `agents/` directories: `context.md`, `current_task.md`, `next_steps.md`, and posting updates to `agents/CHAT.md`). The implementation workflow must follow the Tier 2 sprint fast-track loop: Cypher & Morpheus combine story writing and architecture design into a single document, Mouse generates the plan in `task.md`, Neo implements the features, and Trin performs QA validation.

## 2026-07-15T20:40:24Z

Refactor the `zipmt-rust` codebase to decouple TUI rendering from the compression pipeline, converting the pipeline into a modular library API with dynamic knob adjustments, and upgrading the TUI into an interactive LCARS console with vertical slider columns and mouse/keyboard controls.

Working directory: /home/drusifer/Projects/zipmt
Integrity mode: development

## Requirements

### R1. Front End & UX Engine Abstraction
Abstract the TUI drawing, layout, and event loop into a clean front-end component (e.g., `TuiEngine` or `TuiApp`) that is decoupled from the compression execution logic. This component must consume generic metric streams or state structures, enabling independent testing of the TUI layout and controls via mock metrics without spinning up actual compression threads.

### R2. Compression Pipeline Library Separation
Refactor the compression pipeline code (Split and Stream modes) into a library layout:
- Expose clear progress callbacks or metric streams for byte throughput, worker states, and logger events.
- Provide thread-safe API methods to dynamically alter runtime compression parameters (level, throttle delay) during execution.
- Allow integration with standard Rust logging (e.g., the `log` crate) or local callback buffers so logs can be routed to stderr in non-TUI mode or captured in-memory for TUI rendering.

### R3. CLI Interface and TUI Flag Restoration
- Restore the `-T` / `--tui` CLI argument. TUI mode must *never* run by default; it is strictly opt-in via `-T` / `--tui`.
- Default execution (no `-T`) runs in standard command-line mode, writing progress logs or verbosity to stderr/stdout.
- Disable TUI mode if standard output is redirected/piped when `-T` is specified, to prevent terminal escapes from corrupting piped binary output.

### R4. Star Trek Retro Interactive Controls & Sliders
Upgrade the controls section of the TUI to be a premium, interactive LCARS control panel:
- **Vertical Sliders Section**: Render each adjustable knob (Throttle delay and Compression level) in its own column as a vertical bar showing setting levels, with numbers at the currently set level.
- **Keyboard Navigation**: Pressing `Tab` cycles focus between the knobs (with clear visual highlight for the active knob). Use `+`/`-` or `Up`/`Down` keys to adjust the value of the active knob.
- **Mouse Interaction**: Capture mouse events via Crossterm and enable mouse clicking on the vertical sliders to directly drag or set the slider levels in real-time.

### R5. Decoupled Verification & Snapshot Tests
- Update unit and layout snapshot tests to verify the TUI component in isolation using the testing seam (mock metrics).
- Ensure `make test-rust` compiles and passes cleanly with zero warnings or errors.

## Acceptance Criteria

### CLI Controls
- [ ] Running without `-T` executes compression in standard CLI mode without alternate screen raw mode.
- [ ] Running with `-T` activates alternate screen raw TUI.
- [ ] CLI flags correctly set the startup compression level (default 6) and other options.

### Frontend Decoupling & Seam Testing
- [ ] The TUI rendering is decoupled from compression threads, allowing independent TUI test rendering with mock progress data.
- [ ] The TUI event loop uses the library API to dynamically update active parameters (level, delay) on the pipeline without interrupting compression threads.

### Interactive Slider Board (Knobs)
- [ ] The knobs (Throttle delay and Compression level) are rendered as vertical column bars.
- [ ] Pressing `Tab` toggles active focus between the vertical knobs (visual focus highlight).
- [ ] Pressing `+`/`-` or `Up`/`Down` adjusts the focused knob's value.
- [ ] Clicking on a slider bar with the mouse captures the event and adjusts that knob's value immediately.

## 2026-07-16T01:27:28Z

Resume work at /home/drusifer/Projects/zipmt. Read handoff.md, BRIEFING.md, ORIGINAL_REQUEST.md, and progress.md for current state.
Your parent is 86d04a7e-80b2-41a1-87ac-e679d891a61b — use this ID for all escalation and status reporting (send_message).
Your first task is to spawn an Explorer to recommend a plan to remediate the chronological timeline/provenance violation identified in handoff.md (specifically renaming Trin's verification summary file to the actual chronological time and editing CHAT.md messages #108 and #109), then execute the timeline fixes, and re-run the Forensic Victory Auditor to get a CLEAN verdict.
