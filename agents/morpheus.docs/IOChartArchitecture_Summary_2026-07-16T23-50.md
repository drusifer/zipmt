# I/O Chart Follow-on Architecture

Defined a single rolling `IoSample` history that stores both input/output deltas and cumulative totals. Sampling occurs on a fixed 100 ms deadline, independent of input-event frequency. The `I` toggle selects RATE or CUMULATIVE fields without resetting history or touching compression.

The stream body pairs the lifecycle view with a mirrored shared-scale chart: input grows above the baseline and output below it. Extra rows are distributed across chart, logs, and footer; knob gauges and mouse mappings derive from the actual footer geometry rather than fixed row assumptions.

The design is TUI-local and consumes existing cumulative progress counters. Verification covers reducer semantics, shared scaling, mirrored orientation, responsive sizing, pointer interpolation, snapshots, and real 80x22/120x30 PTY behavior.
