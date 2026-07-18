# Pipeline Flow Architecture Summary

Approved an event-sourced TUI projection using explicit queued, assigned, pending, and written chunk lifecycle events. Runtime chunk size and active worker count are controller atomics; the existing ordered BTreeMap remains authoritative. The worker knob gates a fixed startup pool, while the chunk-size knob affects only future reader blocks. Compression defaults move to a shared level-9 constant.
