# Worker Ratio Moving Average

Each Stream worker now retains a rolling window of its last 10 finalized chunk
ratios. `R` displays the bounded fixed-width average during subsequent
assignments and changes as newly configured chunks complete.

All 42 unit tests, 7 integration tests, documentation tests, snapshots, and
debug build pass.
