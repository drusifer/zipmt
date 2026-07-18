# Graph Buckets and Stream Workers UAT

## Result

Passed.

## Coverage

- Verified graph history advances on consistent one-second buckets.
- Verified rate labels use the ten-second moving average.
- Verified Stream worker rows expose status, chunk, progress, average input
  rate, compression ratio, and ETA.
- Verified unit, integration, snapshot, and debug-build gates.

## Evidence

- 39 unit tests passed.
- 7 integration tests passed.
- Documentation tests and debug build passed.
