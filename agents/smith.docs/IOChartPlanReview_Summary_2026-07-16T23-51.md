# I/O Chart Combined UX Gate

Approved the combined stories and architecture. The mirrored graph matches the operator mental model: input appears above a clear baseline, output below it, and both retain textual labels and a shared scale. `I` provides a discoverable expert shortcut between RATE and CUMULATIVE without creating a fifth knob or resetting history.

One clarification was applied before approval: RATE mode must calculate and label bytes per second from counter deltas and actual elapsed time; CUMULATIVE shows session byte totals. This prevents cadence-dependent values from being mislabeled as rate.

The responsive plan is also approved. The 80x22 layout remains compact, while taller terminals share extra rows between chart, logs, and visibly taller gauges. Mouse coordinates must derive from the rendered footer geometry.

Combined Gate 1/2: APPROVED. Smith must test the real PTY behavior after implementation.
