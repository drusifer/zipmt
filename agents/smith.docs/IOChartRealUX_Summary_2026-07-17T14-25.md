# Mirrored I/O Chart Real-Behavior UX Approval

Tested actual random-data streams at 120x30 and 80x22. Input history scrolls across the upper half and output across the lower half with a clear center baseline. RATE shows honest `/s` units; injected `I` input visibly changed RATE → CUMULATIVE → RATE while the existing graph remained intact.

At 120x30 the chart receives more vertical/horizontal history and the four gauges expand to six content rows. At 80x22 the lifecycle labels, compact graph, logs, four controls, and mode hint all remain readable. Alternate-screen cleanup was clean and temporary outputs/logs were removed.

UX gate: APPROVED.
