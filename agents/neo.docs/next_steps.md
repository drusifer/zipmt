# Next Steps

- Trin should verify runtime Chunk changes are applied only at safe read
  boundaries in both modes.
- Trin should verify log tail-follow, scroll-back, Home/End, and mouse-wheel
  behavior in a real terminal.
- Preserve ownership transfer of the Stream reader `Vec`; do not restore the
  `to_vec()` clone.
- Preserve direct Stream slice-to-encoder input.
- Preserve direct Split encoder-to-temporary-file output.
- Profile XZ memory separately: chunk-copy removal does not reduce liblzma's
  level-dependent dictionary and match-finder working set.
- Compare levels and worker counts with a larger input if throughput scaling,
  rather than hotspot attribution, becomes the next optimization goal.

---
*Last updated: 2026-07-18T17:14:00-04:00*
