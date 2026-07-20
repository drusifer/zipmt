# Rust Refactor Phase 1 PTY UX Blocker

**Status:** Blocked by anti-loop guard

Automated Phase 1 behavior, snapshots, strict Clippy, and TUI complexity pass.
The real-PTY validation has had two unsuccessful approaches:

1. The initial target used normal interactive completion and waited correctly
   for Enter, so automation hung.
2. The target then used `ZIPMT_FORCE_TUI=1` and exited cleanly with valid Xz
   output, but its 455-byte transcripts contain only terminal
   enter/leave/mouse/cursor sequences and no dashboard text. The recipe used
   semicolon chaining without `set -e`, so its failed `rg` assertion was hidden
   by the final successful `echo`.

The likely next approach is to make the input nontrivial (not a sparse zero
file), enforce shell failure propagation, and assert visible dashboard/status
text plus terminal cleanup. This would be the third PTY-validation attempt and
requires user direction under the active anti-loop rule.
