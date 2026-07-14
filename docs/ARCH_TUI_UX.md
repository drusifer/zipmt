# TUI UX Upgrade Architecture (Star Trek LCARS Style)

This document describes the design system, colors, layout structures, and math formulas for the Star Trek LCARS-styled terminal progress monitor.

## Design System & Style Guide

To implement the LCARS aesthetic, we define specific ANSI escape sequence colors:

| LCARS Role | Color | ANSI Code |
| :--- | :--- | :--- |
| Core Header | Orange | `\x1B[38;5;208m` |
| Primary Details | Cyan | `\x1B[38;5;117m` |
| Secondary Accent | Purple | `\x1B[38;5;147m` |
| Status Indicator | Yellow | `\x1B[38;5;220m` |
| Reset | Reset | `\x1B[0m` |

### Structural Layout Blocks
LCARS consoles use brackets, block fills, and code sectors:
- Headers: `╭──────────────────────────────────────────────────╮` or `[=== LCARS DIAGNOSTIC OVERRIDE ===]`
- Sidebars: `|` or `[` with accented panels.

---

## Metric Formulas

### 1. ETA Calculation (Known size)
Let $S_{total}$ be the total size of the input file in bytes.
Let $S_{processed}$ be the current total bytes processed by worker threads.
Let $t$ be the elapsed time in seconds.

$$\text{Throughput} (B/s) = \frac{S_{processed}}{t}$$
$$\text{Remaining Bytes} = S_{total} - S_{processed}$$
$$\text{ETA (seconds)} = \frac{\text{Remaining Bytes}}{\text{Throughput}}$$

*Rules:*
- If $t < 0.2\text{s}$ or $S_{processed} == 0$, display `ETA: Estimating...` to avoid division by zero or extreme speed fluctuations at startup.

### 2. Time-Capacity Projections (Unknown size)
If size is unknown (stdin stream), we forecast capacity limits at the current rate of ingestion:

$$\text{Rate} = \frac{S_{read}}{t}$$
$$\text{Forecast}_{1m} = \text{Rate} \times 60$$
$$\text{Forecast}_{5m} = \text{Rate} \times 300$$
$$\text{Forecast}_{10m} = \text{Rate} \times 600$$
