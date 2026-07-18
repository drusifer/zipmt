# Split TUI Uplift Retrospective

Closed 9/9 tasks with correctness, architecture, and real-terminal UX gates approved.

What worked: evidence-first control semantics prevented false affordances; explicit lifecycle events supported both aggregate and row-level UX; reusing the mirrored chart reduced implementation and cognitive overhead; responsive geometry was validated at both minimum and expanded sizes.

Improvement captured: page movement must derive from the exact rendered capacity, not a parallel terminal-size estimate. Morpheus caught and corrected that mismatch before close.

Tooling debt remains: `make judge-trace` is still missing.
