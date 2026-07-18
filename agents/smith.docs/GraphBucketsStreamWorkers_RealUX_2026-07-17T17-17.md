# Graph Buckets and Stream Workers UX Review

## Decision

Approved through TestBackend and snapshot review.

## Findings

- The magenta moving-average line is distinguishable from the yellow raw bars.
- `MA10s` communicates the smoothing horizon in the graph title.
- Stream worker rows keep status and chunk identity first, followed by
  progress, rate, ratio, and ETA for quick scanning.
- Responsive row capacity and explicit overflow preserve useful behavior on
  smaller and larger terminals.
