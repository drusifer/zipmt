# Worker Statistics Below Gauge

Moved worker AVG, ratio, and ETA onto a dedicated row below the one-cell progress
gauge. Progress, rates, ratio, and ETA now use fixed-width, two-decimal formats.
The completion header retains fixed-point aggregate rates and ratio.

All Rust unit, integration, documentation, snapshot, and debug-build gates pass.
