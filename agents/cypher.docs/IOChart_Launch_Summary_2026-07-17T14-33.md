# Mirrored I/O Chart and Responsive Knobs — Launch

Stream mode now displays smooth scrolling I/O history with input above and output below a shared baseline. RATE reports bytes per second; `I` switches to CUMULATIVE session totals and back without resetting history.

The lifecycle panel remains visible beside the chart. At the 80x22 minimum the graph and controls stay compact; larger terminals provide more chart history, logs, and taller Level, Throttle, Chunk, and Workers gauges with aligned mouse behavior.

Delivery status: 3/3 tasks complete. Focused sampling/render/control tests, snapshots, integration 7/7, architecture review, and real 80x22/120x30 PTY UX tests passed. Residual debt is the missing `make judge-trace` target.
