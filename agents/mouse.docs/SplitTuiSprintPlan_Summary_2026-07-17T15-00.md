# Split TUI Uplift Sprint Plan

Created a three-phase, nine-task board in root `task.md`.

Phase 1 establishes authoritative lifecycle, aggregate, and sampling state with correctness tests. Phase 2 renders the aggregate/sector board, responsive paging, and mirrored chart. Phase 3 removes false control affordances and completes regression plus real-terminal validation.

Every phase has Neo implementation, Trin UAT, and Morpheus review; observable work includes Smith review. The plan explicitly forbids changes to partitioning, Rayon pool sizing, compressor semantics, output format, and ordering.
