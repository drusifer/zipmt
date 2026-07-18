# Pipeline Flow Sprint Retrospective

## What worked

- Typed lifecycle events and a centralized reducer made the queue-to-output UX testable without weakening output-order invariants.
- Short implementation/QA handoffs caught stale snapshots and the truncated status header before closure.
- Bounded tests plus real PTY checks gave fast structural feedback and credible observable-behavior evidence.
- The late responsive-terminal request fit cleanly because layout and controls were already componentized.

## What to improve

- Header and layout calculations should use content-derived widths from the first implementation, including every variable state label.
- Responsive mouse geometry should receive direct coordinate tests in addition to helper boundary tests.
- The documented `make judge-trace` workflow still lacks a Makefile target and should be scheduled as tooling maintenance.

## Outcome

All 9 committed tasks completed. No product blocker remains. The known `/dev/tty` fixture is environment-sensitive, and the missing judge target is tooling debt rather than a release defect.
