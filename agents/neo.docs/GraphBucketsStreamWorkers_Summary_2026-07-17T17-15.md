# Graph Buckets and Stream Workers Summary

Decoupled I/O graph history from the 100 ms UI/system tick. History now emits normalized one-second buckets; the magenta overlay and RATE labels consume ten buckets as `◆MA10s`.

Added Stream worker progress events and a responsive worker board showing one-based chunk, status, percentage, assignment-lifetime average rate, ratio, and ETA. Worker results remain visible as DONE; smaller terminals show explicit range/overflow.

Validation: 39 unit tests, 7 integration tests, updated snapshots, and debug build.
