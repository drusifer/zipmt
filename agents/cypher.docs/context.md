# Agent Local Context (context.md)

This file tracks the current state of product vision, requirements, and decisions made by the PM (Cypher).

## Recent Decisions
- **Formulated Product Strategy for `zipmt-rust`**: Designed the core requirements, non-functional criteria, and safety constraints for a robust Rust-based multi-threaded parallel compression utility.
- **Created PRD**: Authored the Product Requirements Document in [docs/PRD.md](file:///home/drusifer/Projects/zipmt/docs/PRD.md).
- **Created User Stories**: Authored the user stories and acceptance criteria in [docs/USER_STORIES.md](file:///home/drusifer/Projects/zipmt/docs/USER_STORIES.md).

## Key Findings
- **Security & UX Focus**:
  - Reverted the dangerous default behavior in C (which deleted source files by default). The Rust version will keep files by default, requiring an explicit `--delete` flag to prune source material.
  - Mandated zero-panic handlers in decompression/verification blocks to prevent stability vulnerabilities (addressing the Go XZ verification crash).
  - Explicitly called out backpressure and backoff mechanisms to control peak memory footprint during stream operations (limiting loaded blocks to `N * 2` of size 4MB).

## Important Notes
- All documents contain standard `TLDR:` blocks and are successfully registered in the documentation parser.

---
*Last updated: 2026-07-13T13:55:00*
