# Judge Codex Trace Support

**Date:** 2026-07-18 18:27 EDT
**Persona:** Bob

## Outcome

Judge now discovers both Claude Code and Codex CLI sessions and delegates native trace normalization to pinned Tracegate 0.1.6. Its existing anti-pattern rules and report formats remain intact.

## Implementation

- Added `make judge-tools-install` with an isolated `.judge-venv`.
- Added `SOURCE=auto|codex|claude` support to `make judge-trace`.
- Filtered Codex daily rollouts by the session working directory.
- Added a thin adapter for Tracegate events, including Codex code-mode `tools.*` calls.
- Added four focused parser tests.

## Verification

- `make judge-trace-test V=-vvv`: 4/4 passed.
- Real Codex rollout: 805 calls, 9 flags, one session; Markdown report written under `/tmp`.
