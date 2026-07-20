# Rust Refactor Sprint Architecture Ratification

Morpheus ratified `docs/USER_STORIES_RUST_REFACTOR.md` against the completed
analyzer-backed quality review.

The five-stage order is binding: bounded File-to-Stdout correction; typed/pure
TUI seams; Stream then Split orchestration extraction; controller/progress and
codec-loop cleanup; application-shell cleanup. Each stage establishes seams
needed by the next and avoids combining concurrency changes with channel or
algorithm changes.

The required invariants are direct encoder writes, reusable buffer ownership,
bounded queues and memory, ordered output, cancellation and artifact cleanup,
existing control semantics, output compatibility, and no unexplained
throughput regression above 5%.

The combined artifact is ready for Smith's single Tier 2 product/architecture
review.
