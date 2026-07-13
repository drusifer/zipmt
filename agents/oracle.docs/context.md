# Agent Local Context (context.md)

This file tracks the current state of knowledge organization, documentation structure, and recent changes made by the Oracle.

## Recent Decisions
- **Evaluated separate Go Implementation (`zipmt-go`)**: Analyzed the newly merged Go version and documented its architecture, flags, and usage.
- **Side-by-Side Documentation**: Updated `docs/ARCH.md`, `docs/USAGE.md`, `README.md`, and `MINDMAP.md` to cleanly present both C and Go implementations.
- **Identified Critical Go Bugs**: Logged a critical copy-ordering bug in `ZipWriter.Write` (causing buffer zeroing) and a nil-pointer verify bug in XZ compressor, documenting them as major architectural risks.
- **Re-Initialized Bob Protocol discovery links**: Ran `setup_agent_links.py` to confirm that all 9 personas and 13 shared skills are discoverable by development tools.
- **Brought State Files to Protocol Compliance**: Generated task summaries under `agents/oracle.docs/` using hyphenated timestamps (`YYYY-mm-ddTHH-MM.md`) to exclude colons for filesystem safety.

## Key Findings
- **Discovery links verified**:
  - Found 9 active personas and 13 shared skills. Symlinks are correctly registered.
  - Capability configs are fully aligned in `PROJECT.md`.
- **Go Version Data Corruption Bug**:
  - Located in `zipmt-go/zipmt/zipwriter.go:38` (`copy(data[start:end], chunk)`). It overwrites the input data with zeros and outputs a compressed stream of all zeros.
- **Go Version XZ Verification Panic Bug**:
  - Located in `zipmt-go/zipmt/xzzipper.go:27-29` (`if err != nil { err = reader.Verify() }`). Attempts to verify on a `nil` reader if an initialization error occurs, triggering a nil-pointer panic.

## Important Notes
- **TLDR compliance**: Added a standard `TLDR:` block to `zipmt-go/README.md`. All files are registered in the documentation index and discoverable via `make tldr`.
- **Task Summaries written**:
  - `GenerateDocs_Summary_2026-07-12T11-22.md`
  - `EvalGoImpl_Summary_2026-07-13T13-39.md`
  - `BobProtocolInit_Summary_2026-07-13T13-43.md`
  - `SummaryReview_Summary_2026-07-13T13-44.md`

---
*Last updated: 2026-07-13T13:43:00*
