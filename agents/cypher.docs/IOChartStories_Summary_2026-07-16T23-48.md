# I/O Chart Follow-on Stories

Defined a mirrored stream-mode history chart with bytes in above and bytes out below a shared baseline. The visible `I` toggle switches between RATE samples (per-refresh deltas) and CUMULATIVE session totals without resetting history. Both modes scroll oldest-left to newest-right, share one honest scale, preserve final history, and expand with the terminal.

The queue/worker/pending/output flow, four controls, one-based identities, and 80x22 minimum remain required. Deterministic reducer/render coverage and a real PTY smooth-scrolling check are acceptance gates.

On taller terminals, extra rows are shared intentionally between the chart, logs, and taller knob gauges. A 30-row terminal must provide visibly more gauge space than the compact 22-row layout while keeping mouse and keyboard behavior aligned.

Handed to Morpheus to complete the fast-track architecture in `docs/IO_CHART_SPRINT.md`.
