# BRIEFING — 2026-07-16T01:17:42Z

## Mission
Fix CLI opt-in TUI requirement gap in zipmt-rust to make TUI mode strictly opt-in and respect environment/redirection overrides.

## 🔒 My Identity
- Archetype: Neo
- Roles: implementer, qa, specialist
- Working directory: /home/drusifer/Projects/zipmt/.agents/teamwork_preview_worker_neo/
- Original parent: 4f190386-9506-47ca-bfe8-700c1e5de91b
- Milestone: TUI Flag Restoration

## 🔒 Key Constraints
- TUI mode must be strictly opt-in via -T/--tui and must never run by default.
- Respect ZIPMT_FORCE_TUI environment variable override to true.
- Respect TUI capability checks (e.g. redirected streams, not TTY), override to false even if -T is passed.
- No "while I'm here" refactoring.
- Run build/tests before concluding.

## Current Parent
- Conversation ID: 4f190386-9506-47ca-bfe8-700c1e5de91b
- Updated: not yet

## Task Summary
- **What to build**: Update `run_tui` logic in `zipmt-rust/src/main.rs` and update `test_integration_tui_mode` in `zipmt-rust/tests/integration_test.rs`.
- **Success criteria**: Code compiles with zero warnings/errors and passes all tests; TUI is strictly opt-in, override works, non-TTY fallback works.
- **Interface contracts**: CLI args in main.rs and integration tests.
- **Code layout**: zipmt-rust/src/main.rs and zipmt-rust/tests/integration_test.rs.

## Key Decisions Made
- [initial decision] Implement strictly opt-in check: check force environment first, then check capability, then check argument.

## Artifact Index
- /home/drusifer/Projects/zipmt/.agents/teamwork_preview_worker_neo/handoff.md — Handoff report
- /home/drusifer/Projects/zipmt/.agents/teamwork_preview_worker_neo/progress.md — Progress tracker
- /home/drusifer/Projects/zipmt/.agents/teamwork_preview_worker_neo/BRIEFING.md — Briefing file
