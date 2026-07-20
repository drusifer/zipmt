# Rust Quality Tooling

The Rust quality workflow is exposed through Make targets so local runs and CI
use the same commands.

## Core gate

Run:

```sh
make rust-quality
```

This checks formatting, strict Clippy findings, cognitive complexity, exported
cyclomatic metrics, and compiler dead-code warnings.

Cyclomatic, cognitive, Halstead, and maintainability metrics are written to
`build/rust-code-metrics.json` by:

```sh
make rust-cyclomatic
```

The cognitive-complexity limit is 20 and is configured in
`zipmt-rust/clippy.toml`.

## Security and deeper analysis

```sh
make rust-audit
make rust-unsafe
make rust-bloat
make rust-quality-full
```

`rust-audit` combines RustSec advisories with cargo-deny license, dependency,
and source checks. `rust-unsafe` reports unsafe usage across the dependency
graph. `rust-bloat` reports release binary size by crate.

## Memory diagnostics and profiling

```sh
make rust-miri
make rust-memcheck MEMCHECK_ARGS="-o /tmp/out.xz /path/to/input"
make rust-profile PROFILE_ARGS="-o /tmp/out.xz /path/to/input"

# Build and benchmark the current revision, then append one history record.
make rust-benchmark RUNS=3
```

The first run creates a stable 32 MiB workload at
`build/rust-benchmark-input.bin`. Every invocation reuses that workload and
appends a timestamped, revision-tagged record to
`benchmarks/rust-history.yaml`, including dirty-worktree state, workload hash,
mean time, and throughput. Commit the history file to compare revisions over
time; keep the generated workload in `build/` on the benchmark host. The
history is seeded with the sprint-closing measurements; their workload hash is
marked unavailable because that temporary input predated the retained fixture.

Miri requires the nightly Miri component. Memcheck requires Valgrind.
Flamegraphs require `cargo-flamegraph` and the platform profiler (`perf` on
Linux). Use a representative, non-sensitive workload for memory and profiling
runs.

## Tool bootstrap

```sh
make rust-tools-install
make rust-tools-check
```

The install target installs Cargo-based tools. Valgrind, perf, and nightly Miri
remain explicit platform/toolchain prerequisites.

## Initial baseline (2026-07-18)

- Formatting passes.
- Compiler dead-code checks pass.
- Strict Clippy reports 22 existing findings.
- Cognitive complexity exceeds 20 in three production paths: 21 in split mode,
  and 27 and 22 in stream mode.
- RustSec advisories and cargo-deny license/source policy pass. Duplicate
  transitive dependency versions remain warnings.
- cargo-geiger completes its scan but exits non-zero because it cannot parse
  several dependency/support files with its current parser.
