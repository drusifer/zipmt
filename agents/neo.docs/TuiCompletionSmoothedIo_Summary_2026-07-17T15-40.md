# TUI Completion and Smoothed I/O Summary

Successful interactive TUIs now freeze their elapsed time and telemetry, retain the COMPLETE dashboard, show final bytes/ratio/average throughput, and close with Enter/Q/Esc. Forced-TUI automation renders completion and exits without waiting.

RATE charts now overlay a five-sample moving average for both mirrored series and use the latest smoothed values in IN/OUT labels. CUMULATIVE remains exact. Updated snapshots and deterministic tests cover smoothing, final statistics, and frozen elapsed time.
