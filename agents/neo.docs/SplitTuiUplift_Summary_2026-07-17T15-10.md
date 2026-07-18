# Split TUI Uplift Implementation Summary

Implemented explicit typed Split sector stages at waiting, running, and done transitions without changing Rayon work or output assembly. Added centralized bounded reduction, aggregate progress/count/ETA and completed-only ratio helpers, and generalized mirrored RATE/CUMULATIVE I/O sampling to Split counters.

Replaced the legacy zero-based sector/speed view with a responsive one-based lifecycle board, aggregate summary, visible range/overflow, PgUp/PgDn state, and the shared mirrored input/output chart. Split focus and pointer behavior now expose only Throttle; Level, Partition, and Pool render as fixed while Stream retains all controls.

Added lifecycle, aggregate, idle-sample, focus, responsive render, and byte-perfect integration coverage. The complete Rust suite passes serially: 31 unit tests and 7 integration tests.
