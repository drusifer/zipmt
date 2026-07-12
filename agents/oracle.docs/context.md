# Agent Local Context (context.md)

This file tracks the current state of knowledge organization, documentation structure, and recent changes made by the Oracle.

## Recent Decisions
- **Generated global documentation layout for zipmt**: Formulated a structured docs framework consisting of user-facing guides, detailed architecture specs, decisions records, lessons learned logs, repository mindmaps, and a root entry point.
- **Fixed `make tldr` target**: Modified `Makefile` to target `TLDR:` instead of `TL;DR:` to match the actual marker used in standard template files, enabling ripgrep to correctly parse project summaries.

## Key Findings
- **Missing Build Dependencies**:
  - Found that building the `zipmt` tool requires `-dev` versions of headers (`bzlib.h` via `libbz2-dev`), which was not documented or pre-installed, leading to immediate compilation failures. This has been added to user instructions in `docs/USAGE.md`.
- **Destructive Default Behavior**:
  - Identified that `zipmt` deletes original source files by default when compression finishes. Clear safety warnings were placed in `README.md`, `docs/USAGE.md`, and `LESSONS.md`.
- **GLib Deprecated Calls**:
  - Noted that `zipmt.c` invokes `g_thread_init()`, which is deprecated in modern GLib versions. This was documented as technical debt in `docs/ARCH.md` and `LESSONS.md`.

## Important Notes
- **Repository documentation is fully indexed**: All new files are registered under `/home/drusifer/Projects/zipmt/agents/DOCUMENTATION_INDEX.md`.
- **TLDR discoverability**: All new files (`README.md`, `MINDMAP.md`, `DECISIONS.md`, `LESSONS.md`, `docs/ARCH.md`, `docs/USAGE.md`) contain standard `TLDR:` blocks and successfully report through the updated `make tldr` tool.

---
*Last updated: 2026-07-12T11:22:00*
