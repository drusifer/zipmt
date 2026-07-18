# Split-mode TUI Uplift Stories

Defined five stories covering aggregate whole-job progress, explicit one-based sector lifecycles, mirrored RATE/CUMULATIVE I/O history, truthful fixed-vs-live controls, and responsive paging/layout.

The sprint explicitly avoids runtime repartitioning or Rayon-pool resizing. Partition and Pool controls must be visibly fixed in Split mode, while Stream retains its four live knobs. Deterministic and real 80x22/120x30 UX gates are mandatory.
