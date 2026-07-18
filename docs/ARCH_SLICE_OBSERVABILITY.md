# Slice Observability and System Telemetry Architecture

- Extend each `StripeProgress` with start/completion instants. Derive average input/output rates from authoritative bytes and active duration; do not add worker-side timers or locks.
- Derive composite progress from the reducer state. ETA uses aggregate processed bytes divided by frozen/current job elapsed, then remaining bytes divided by that average.
- Add TUI-local process telemetry sampled on the existing fixed cadence. Linux reads `/proc/self/stat` for CPU ticks and `/proc/self/status` for `VmRSS`; unsupported or malformed sources produce unavailable values.
- Split rendering separates a compact composite header from detailed one-row slice widgets. The right column stacks the mirrored I/O chart above a shared System panel.
- Stream retains its flow panel and uses the same right-side chart/System stack.
- Telemetry is observational and must not change compression threads, section storage, or event ordering.
- Graph history uses a separate one-second cadence. RATE smoothing consumes ten buckets for a true ten-second window; the 100 ms cadence remains dedicated to UI responsiveness and process telemetry.
- Stream compressor callbacks emit worker-local input/output progress. `WorkerState` resets timing on assignment, freezes it on completion, and retains the last chunk metrics after returning idle. Rendering derives rate, ratio, and ETA without worker-side locks.
