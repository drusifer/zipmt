# Pipeline Flow Phase 2 Implementation

Added validated atomic chunk size (64 KiB–8 MiB powers of two, 1 MiB default) and active workers (1..max, all active by default) to `PipelineController`. The reader loads size before each future block. Fixed-pool workers publish ON/OFF availability, avoid new assignments while disabled, and requeue an untouched block if eligibility changes during receive. Reader completion cleanly releases enabled and disabled workers.

Verification: controller boundaries 2/2, mid-stream control/integrity/order 1/1, existing stream compression/lifecycle 1/1.
