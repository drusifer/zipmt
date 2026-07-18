# Compact Stream Worker Cards

Implemented bordered three-row cards for Stream workers. Each card keeps worker,
status, and chunk identity in its title and renders a one-cell-high progress
gauge labeled with fixed AVG, ratio, ETA, and percentage fields.

The 80x22 layout shows two complete cards; larger terminals expose additional
cards in three-row increments. Rust unit, integration, documentation, snapshot,
and debug-build gates pass.
