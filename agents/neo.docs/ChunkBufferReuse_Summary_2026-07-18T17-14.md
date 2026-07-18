# Chunk Buffer Reuse Summary

## Implementation

- Stream reader truncates and transfers its existing chunk allocation instead
  of cloning it before queueing.
- Stream workers pass the owned chunk slice directly to each encoder rather
  than copying through the reader-oriented compression buffer.
- Split workers allocate one heap-backed chunk and reuse it for their entire
  bounded source range.
- The existing atomic Chunk control now governs both Stream chunk allocation
  and Split worker read chunks.
- Split workers load the size at read boundaries with acquire semantics. When
  it changes, the worker replaces the old allocation before the next read; no
  resize occurs while the encoder borrows the populated slice.
- Split encoder output continues writing directly through `CountingWriter` to
  the worker's temporary slice file.
- Added bounded verbose Split diagnostics for worker ranges, initial and
  replacement chunk sizes, temporary destinations, approximately 10% progress
  milestones, encoder completion, and ordered final concatenation.
- Log view now follows the newest messages by default, scrolls backward/forward
  with unfocused Up/Down or the mouse wheel over the panel, and jumps to
  oldest/newest with Home/End.

## Verification

- Added coverage proving a Stream chunk produces one direct encoder feed.
- Added coverage proving a Split reader observes 64 KiB, then replaces its
  allocation with 128 KiB after an atomic runtime update.
- Initial unit run: 43 passed; two expected Split UX assertions failed because
  Partition was formerly fixed.
- Updated the live Chunk control contract and snapshot.
- Both formerly failing focused tests pass.
- Integration suite: 7 passed.
- Focused Split compression test passes after diagnostic logging changes.
- Log-window unit coverage and both updated TUI layout snapshots pass.

## Native Performance Profile

- Added release debug symbols and reproducible `rust-profile-report` and
  Callgrind fallback Make targets.
- Profiled a 16 MiB Split workload with two XZ level-1 workers.
- Native perf captured 1,956 samples and lost one data chunk.
- Top self-costs are entirely liblzma: `lzma_mf_hc4_find` 25.94%,
  `hc_find_func` 25.48%, `lzma_lzma_encode` 18.42%, and
  `lzma_mf_hc4_skip` 17.45% (87.29% combined).
- No application-level Split orchestration, chunk replacement, progress
  logging, or final concatenation function appears as a material self-cost.
- Output passed `xz --test`.
- Added a reproducible stdin-driven `rust-profile-stream` target with distinct
  Split and Stream flamegraph/report artifacts.
- The matching Stream run captured 1,597 samples and lost four data chunks.
  Its top liblzma costs were `lzma_mf_hc4_find` 26.09%, `hc_find_func` 21.59%,
  `lzma_lzma_encode` 20.15%, and `lzma_mf_hc4_skip` 20.07% (87.90% combined).
- Stream queueing, direct chunk input, ordered output buffering, and writing do
  not appear as material self-costs. Stream output also passed `xz --test`.
