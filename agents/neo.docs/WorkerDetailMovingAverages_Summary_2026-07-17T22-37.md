# Worker Detail Moving Averages

Worker input throughput now uses a 10-chunk window. During an assignment its
live rate joins up to nine finalized samples; when idle only finalized history
is used. The same smoothed rate drives AVG and ETA. Ratio remains a 10-finalized-
chunk average.

All 43 unit tests, 7 integration tests, documentation tests, snapshots, and
debug build pass.
