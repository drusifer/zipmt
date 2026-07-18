# Mirrored I/O Chart Phase 1 Implementation

Implemented fixed-cadence stream sampling into one rolling history containing input/output bytes-per-second rates and cumulative totals. `I` switches RATE/CUMULATIVE without clearing samples or changing compression.

Added a responsive mirrored shared-scale chart with input above the baseline, output below, oldest samples left, and newest right. The stream lifecycle view remains beside it. Terminal height now distributes extra rows across chart, logs, and footer; taller knob gauges and mouse mappings derive from the rendered footer height.

Added the missing `make build-rust-debug` and `make format-rust` automation targets. Focused sampling, orientation, resize, stream render, mouse mapping, and snapshot checks pass; integration passes 7/7. A real 120x30 PTY stream showed smooth scrolling input/output traces and six-row knob gauges. Temporary PTY artifacts were removed.
