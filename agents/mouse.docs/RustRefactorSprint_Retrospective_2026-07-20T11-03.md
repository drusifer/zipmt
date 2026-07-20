# Rust Boundary Refactor Sprint Retrospective

## Outcome

All 11 sprint tasks and the Definition of Done closed. The refactor preserved
behavior and improved the measured 32 MiB XZ workload by 1.15% against the
pre-sprint baseline.

## What worked

- Short, analyzer-directed extraction increments kept behavioral changes small.
- Focused tests preceded full-suite and deterministic quality gates.
- Real user testing at 80x22 found the missing bounded-panel navigation.
- Profiles confirmed liblzma remained the dominant CPU cost.
- Unnamed Split temporary files strengthened cleanup under process termination.

## Lessons

- Performance acceptance criteria need a retained workload and revision-tagged
  numeric history, not only flamegraphs.
- Generated YAML must be validated after an actual append; the first writer
  escaped newlines incorrectly and was corrected within the bounded loop.
- A benchmark record from a dirty tree is useful during development, but a
  clean committed revision should be recorded before release.

## Follow-up convention

Run `make rust-benchmark RUNS=3` on the same benchmark host after meaningful
hot-path changes and before release. Commit `benchmarks/rust-history.yaml`;
retain `build/rust-benchmark-input.bin` on that host so its SHA-256 stays
constant across revisions.
