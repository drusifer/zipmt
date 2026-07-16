# Project: zipmt-rust Decoupling & Interactive TUI Upgrade

## Architecture
- `zipmt-rust` codebase is refactored into a decoupled architecture:
  - **Modular Pipeline Library**: Extraction of compression execution (Split/Stream) into a separate module/API, exposing metrics stream and thread-safe APIs to update throttling/compression parameters at runtime.
  - **Decoupled TuiEngine**: Front-end component that consumes generic metrics/state and handles Crossterm main-loop events, keyboard focus, and mouse drag inputs.
  - **CLI Layer**: Restores `-T` flag, defaulting to standard CLI progress to stderr unless `-T` is specified, and handles output redirection safely.

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M1 | Combined Story & Arch | Cypher & Morpheus draft stories and architecture design | None | DONE |
| M2 | Review & Sprint Plan | Smith reviews story/arch, Mouse plans tasks in `task.md` | M1 | DONE |
| M3 | Neo Implementation | Refactor codebase, implement backend/frontend, controls, restore `-T` | M2 | PLANNED |
| M4 | Trin QA Validation | UAT validation of features, layout verification, and test execution | M3 | PLANNED |
| M5 | Forensic Audit | Forensic Auditor verifies implementation integrity and protocol compliance | M4 | PLANNED |

## Interface Contracts
### Pipeline Library ↔ Front End / CLI
- Progress callback / metric streams: throughput, active workers, bytes read/written.
- Thread-safe parameter updates: `set_compression_level(level)` and `set_throttle_delay(delay)`.
- Fallback checks: disable TUI if stdout is redirected or not a TTY.
