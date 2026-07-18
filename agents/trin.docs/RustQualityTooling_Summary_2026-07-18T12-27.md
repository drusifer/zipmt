# Rust Quality Tooling Summary

## Outcome

Added Make-driven Rust formatting, linting, complexity, dead-code, dependency
security, unsafe-code, memory-diagnostic, binary-size, and profiling workflows.
Cyclomatic metrics are exported to `build/rust-code-metrics.json`.

## Validation

- `make rust-tools-check`: Cargo tools installed; Valgrind, perf, and Miri absent.
- `make rust-format-check`: passed.
- `make rust-cyclomatic`: passed and generated JSON metrics.
- `make rust-dead-code`: passed.
- `make rust-complexity`: found three production paths above threshold 20.
- `make rust-quality`: found 22 existing strict Clippy findings.
- `make rust-audit`: RustSec and cargo-deny passed after the project declared
  its MIT license; duplicate transitive versions remain warnings.
- `make rust-unsafe`: scan completed but cargo-geiger returned non-zero for
  parser warnings in dependency/support files.

## Follow-up

The quality gates intentionally remain red where they expose existing debt.
Do not suppress scanner findings without a product decision. Install and
exercise Miri, Valgrind, and perf on a suitable host.
