# Split TUI Uplift Architecture Summary

Defined an observational Split-mode architecture with explicit `Waiting`/`Running`/`Done` sector events, a centralized TUI reducer, pure aggregate helpers, responsive sector paging, and reuse of the mirrored I/O history.

The compressor audit established that level is loaded once when each sector encoder is created. Split mode must therefore present Level as `ENCODER FIXED`, Chunk as `PARTITION FIXED`, and Workers as `POOL FIXED`; only Pause and Throttle are live. Aggregate ratio uses completed sectors only so partially processed input is not compared with output that does not yet exist.

The design preserves Rayon execution, sequential output assembly, file format, and Stream behavior. Verification includes deterministic state/geometry tests, byte-perfect integration, responsive snapshots, and real PTY UX at 80x22 and 120x30.
