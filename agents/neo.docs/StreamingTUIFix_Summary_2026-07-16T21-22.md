# Streaming TUI Regression Fix

## Cause

`run_app` required stdin and stdout to both be terminals before honoring `-T`. A command that pipes an archive into stdin therefore always disabled TUI mode, even though Ratatui/Crossterm renders exclusively on stderr.

## Fix

- Base interactive eligibility on `stderr().is_terminal()`.
- Allow stdin and compression output to be pipes/files while stderr owns the dashboard.
- Preserve safe fallback when stderr is redirected.
- Make `--no-tui` override CLI and environment requests.
- Allow `make test-rust ARGS="..."` for bounded Rust test selection.

## Verification

- Binary selector tests: 4 passed.
- Integration tests: 7 passed.
- Full unfiltered suite: 12 passed, 1 unrelated failure because the existing terminal-size test cannot open `/dev/tty` in the execution harness. The same failure repeated with PTY allocation, so no third retry was attempted.
