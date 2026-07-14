# Full-Screen LCARS Monitor & Throttling Architecture

This document describes the architectural layout, thread control synchronization, and historical graph models for the upgraded TUI.

## Thread Control & Throttling Synchronization

To implement real-time pausing and throughput deceleration without rebuilding the thread pools, we introduce global thread safety atomic flags in `src/main.rs`:

```rust
// In src/main.rs:
pub static THROTTLE_DELAY_MS: AtomicU64 = AtomicU64::new(0);
pub static IS_PAUSED: AtomicBool = AtomicBool::new(false);
```

### Throttling Interceptor
Inside the incremental compression loop in [compressor.rs](file:///home/drusifer/Projects/zipmt/zipmt-rust/src/compressor.rs), worker threads query these flags on every 64KB block boundary:

```rust
// Before compressing/writing a chunk:
while crate::IS_PAUSED.load(std::sync::atomic::Ordering::Relaxed) {
    std::thread::sleep(std::time::Duration::from_millis(50));
}
let delay = crate::THROTTLE_DELAY_MS.load(std::sync::atomic::Ordering::Relaxed);
if delay > 0 {
    std::thread::sleep(std::time::Duration::from_millis(delay));
}
```

This yields the thread back to the OS scheduler, instantly freeing CPU resources for other user processes when "Slow Down" is toggled.

---

## TUI Layout & History Buffering

### Alternate Buffer & Event Loop
1. **Alternate screen**: On TUI activation, the TUI thread puts the console into raw mode and enters the alternate screen buffer using `crossterm::terminal::enter_alternate_screen()`.
2. **Keyboard listener**: Spawns a dedicated keyboard polling loop checking for key inputs using `crossterm::event::poll`.
3. **History Buffer**: The TuiState tracks a rolling buffer of the last 30 throughput measurements (MB/s).
4. **Historical Graph Render**:
   - Computes the maximum value in the history buffer to scale the graph.
   - Maps each historical value to one of 8 block height characters: ` ` (0), `▂` (1), `▃` (2), `▄` (3), `▅` (4), `▆` (5), `▇` (6), `█` (7).
   - Renders the graph vertically inside the grid panel.
5. **Decoupled Snapshot Testability**: To keep unit tests stable, snapshot tests mock the history buffer and use the plain-text parser to ensure rendering coordinates align.
