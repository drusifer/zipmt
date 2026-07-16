# Technical Architecture Summary — 2026-07-15T16:42:00

## Role: Tech Lead (Morpheus)

### Work Performed
Designed the technical architecture, interface contracts, module layout, and sequence designs for decoupling the TUI from the parallel compression pipeline. Integrated this technical spec as the architecture part of `docs/USER_STORIES_RATATUI_UPGRADE.md`.

### Architecture Highlights
1. **Module Reorganization**:
   - `src/pipeline.rs` will contain the core execution orchestrator `CompressionPipeline` and control handles `PipelineController`.
   - `src/tui/` will host componentized drawing functions and vertical slider widgets.
2. **Channel-Based Metrics Stream**:
   - Spawns worker threads and publishes progress updates to a thread-safe MPSC channel as a sequence of `ProgressEvent`s.
   - The TUI reads from the receiver channel non-blockingly during event ticks to update its local display state.
3. **Encapsulated Thread Control**:
   - Shared atomic flags for pausing, aborting, levels, and speed throttles are grouped under `PipelineController`.
   - Mutating control parameters via the controller propagates changes to the pipeline execution loops at block boundaries without polluting `main.rs` with globals.
4. **Interactive UI Event Mapping**:
   - Implements Tab-key visual focus transitions between compression sliders.
   - Coordinates keyboard (Up/Down) adjustments and mouse coordinates mappings via Crossterm mouse capture.
5. **Decoupled Snapshots**:
   - Renders layout templates from in-memory mock datasets via Ratatui `TestBackend` to prevent resource-heavy integration pipelines during layout verification.
