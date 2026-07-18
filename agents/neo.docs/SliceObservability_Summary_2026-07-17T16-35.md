# Slice Observability Summary

Added authoritative per-slice timing with frozen average input/output rates, output bytes, ratio, lifecycle, progress, and two-line responsive widgets. The distinct composite group now reports aggregate bytes/counts/rates/ratio and derives ETA from whole-job average throughput.

Added fixed-cadence process CPU and RSS collection with tested Linux parsers and unavailable fallback. Both modes stack a Process panel beneath the mirrored chart. The body gained vertical space while retaining the 80x22 contract and expanding at 120x30.

Validation: 37 unit tests, 7 integration tests, updated Split/Stream snapshots, debug build, and actual 80x22/120x30 sessions.
