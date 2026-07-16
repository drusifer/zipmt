# Morpheus Sprint Architecture Summary: CLI TUI Defaulting & Fallback Checks

**Timestamp:** 2026-07-15T16:30:00Z
**Sprint/User Request:** R3 - TUI Defaulting and Fallbacks
**Author:** Morpheus (Tech Lead)

## Technical Design Overview

We have integrated the technical architecture details under the section **### 3. Default TUI & Fallback TTY Detection** in `docs/USER_STORIES_RATATUI.md`.

### Core Architectural Decisions:
1. **Removing CLI options**: Confirmed the elimination of the `-T` / `--tui` CLI flag from Clap `Args`.
2. **Standard Library `IsTerminal` Trait**: Adopted Rust's standard `std::io::IsTerminal` trait on `stdin`/`stdout` for detecting interactive terminal sessions.
3. **Condition Matrix**:
   - `args.stdout` (via `-c`) or default stdout streaming (when stdin is read and output path is `None`) will automatically force `run_tui` to `false`.
   - `!stdout.is_terminal()` will force `run_tui` to `false`.
   - If reading from standard input and `!stdin.is_terminal()`, force `run_tui` to `false`.
   - Otherwise, `run_tui` resolves to `true` by default.
4. **Environment Override**: Provided `ZIPMT_FORCE_TUI` environment check fallback for automated testing and override environments.
5. **Separation of Terminal Modes**: Clean division between the alternate screen initialization path and standard raw streaming paths to prevent terminal pollution.

## Next Steps
- Deliver architecture review documentation to Smith for user approval.
- Work with Mouse for TUI defaulting implementation tasks.
