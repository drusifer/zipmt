# Mirrored I/O Chart Sprint Retrospective

- Product/UX: Mirrored orientation and one visible mode shortcut matched the user request without adding a fifth control card.
- Architecture: Keeping rate and cumulative fields in one sample avoided reset/reconstruction complexity.
- Engineering: Fixed-cadence sampling and pure rendering kept the feature testable; adding Make targets improved repeatability.
- QA: Deterministic orientation/mode tests plus real PTY key injection covered both logic and observable behavior.
- Improvement: Add the still-missing `make judge-trace` target in a maintenance sprint.

Outcome: 3/3 tasks complete with no product blocker.
