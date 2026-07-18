# Current Task

**Status:** Completed / ready for UAT
**Assigned to:** Neo

Unified the existing Chunk control across Stream allocations and reusable Split
read chunks, eliminated the Stream reader clone and intermediate encoder read
copy, retained direct Split encoder-to-temp-file output, and added bounded
worker lifecycle/progress/concatenation diagnostics. The TUI log panel now
follows the tail and supports keyboard and mouse history navigation. Native
Split and Stream profiling confirms liblzma dominates CPU cost.

---
*Last updated: 2026-07-18T17:14:00-04:00*
