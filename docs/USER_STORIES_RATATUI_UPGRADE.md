# User Stories, Acceptance Criteria & Architecture: Interactive TUI & Decoupled Compression Pipeline

This document outlines the product requirements (Cypher) and technical architecture (Morpheus) for decoupling the front-end rendering from the parallel compression pipeline, introducing a modular library API, upgrading the TUI to an interactive Star Trek LCARS dashboard with vertical sliders (supporting keyboard focus/navigation and mouse click/drag adjustments), restoring the `-T` flag, and setting up decoupled layout snapshot tests.

---

## 🎯 Sprint Goal
Decouple TUI rendering from compression logic by establishing a channel-based metrics event stream, convert the compression logic into a modular library API with thread-safe runtime controls (`PipelineController`), restore the `-T` CLI flag with fallback safety, implement interactive vertical sliders for compression level and throttle tuning (with Tab focus and Crossterm mouse tracking), and migrate snapshot tests to use mock states.

---

## 📖 User Stories (Cypher)

### Story 1: Front-end Abstraction (R1)
- **As an** operator of the utility
- **I want** the user interface (TUI or CLI output printer) to be completely independent from the raw compression execution thread pools
- **So that** the compression process remains stable, non-blocking, and reusable across multiple front-ends (including headless/web environments).

#### Acceptance Criteria
1. **Event-Driven Progress**: The compression execution module must not directly reference or lock TUI-specific structs (e.g. `TuiState`). Instead, it publishes progress updates using a generic metrics/progress event stream.
2. **Decoupled State**: The front-end maintains its own local representation of the visual dashboard, updating it solely by consuming the progress event stream.
3. **No Direct Thread Control by TUI**: The TUI rendering functions run completely read-only relative to the active compression thread handles.

---

### Story 2: Modular Pipeline Library API (R2)
- **As a** developer integrating `zipmt-rust`
- **I want** the compression pipeline to be packaged as a modular library API that exposes a cloneable, thread-safe controller
- **So that** I can trigger compression programmatically and adjust compression level, pause state, and throttle delay dynamically at runtime.

#### Acceptance Criteria
1. **Library Interface**: Convert the compression pipeline into a struct (`CompressionPipeline`) in a library module.
2. **Thread-Safe Controller**: Expose a `PipelineController` handle that can be cloned and shared across threads. The controller supports the following thread-safe methods:
   - `update_level(level: u32)`: Sets the compression level dynamically (range 1-9).
   - `update_throttle(delay_ms: u64)`: Sets the speed throttle delay dynamically (range 0-500ms).
   - `pause()`: Suspends compression read/write loops and worker progress.
   - `resume()`: Resumes compression progress.
   - `abort()`: Shuts down all workers immediately, stops execution, and triggers file cleanup.
3. **Encapsulation**: All synchronization primitives (e.g. `Arc`, atomic variables, channels) used to control worker loops must be encapsulated inside the library API rather than utilizing global statics in `main.rs`.

---

### Story 3: CLI and -T Flag Restoration (R3)
- **As a** CLI power user
- **I want** the `-T` / `--tui` flag restored as a command-line option
- **So that** I can explicitly enable or disable the interactive terminal interface, while still retaining the automated defaulting and redirection fallback behaviors.

#### Acceptance Criteria
1. **Restored CLI Option**: Re-add the `-T` / `--tui` boolean flag in the Clap `Args` parser structure.
2. **Explicit Selection Override**:
   - If `-T` is explicitly provided, force the program to run in TUI mode (unless stdout is redirected or the terminal is non-TTY, in which case it must fall back to standard raw logging to prevent stdout stream corruption).
   - If `--no-tui` or `-T=false` can be configured, force standard raw mode.
3. **Smart Defaulting & Fallbacks**: If `-T` is omitted:
   - Run TUI by default for all normal file or stream compressions.
   - Automatically disable TUI and fall back to plain-text verbose stderr log output if standard output is redirected (such as `-c` stdout compression, pipes, or file redirection) or if either stdin or stdout is not a TTY (interactive terminal), verified via `std::io::IsTerminal`.

---

### Story 4: Star Trek LCARS Interactive TUI (R4)
- **As an** operator monitoring a compression run
- **I want** to interactively adjust compression configurations using vertical slider columns, keyboard Tab-focus cycling, and mouse click/drag controls
- **So that** I can easily balance CPU utilization and compression ratio in real time.

#### Acceptance Criteria
1. **Vertical Slider Layout**: Draw two visual vertical sliders in the LCARS Control Panel:
   - **Compression Level**: Ranges from 1 (Fastest) to 9 (Best).
   - **Throttle Delay**: Ranges from 0ms (Uncapped) to 500ms (Heavy Throttle).
2. **Keyboard Navigation & Tuning**:
   - The `Tab` key cycles visual focus: Focus: Compression Level Slider $\leftrightarrow$ Throttle Delay Slider $\leftrightarrow$ Main Status/Logs.
   - The focused slider must be clearly highlighted (e.g. displaying orange/yellow borders or indicator brackets).
   - `Up` / `Down` arrow keys adjust the value of the currently focused slider:
     - Level: Up increases by 1 (max 9), Down decreases by 1 (min 1).
     - Throttle: Up increases delay (+50ms, max 500ms), Down decreases delay (-50ms, min 0ms).
   - Global keystroke overrides (`[` / `]` for level, `+` / `-` for throttle delay) continue to work regardless of which slider is focused.
3. **Mouse Click & Drag**:
   - Enable Crossterm mouse event capture (`crossterm::event::EnableMouseCapture`).
   - Clicking or dragging within a vertical slider's layout box must immediately calculate the corresponding target value and update the compression pipeline via the `PipelineController`.

---

### Story 5: Decoupled Layout Snapshot Tests (R5)
- **As a** CI pipeline verification script
- **I want** the TUI layout snapshot tests to run completely decoupled from compression threads and file system access
- **So that** layout regression tests run in microseconds and remain extremely reliable.

#### Acceptance Criteria
1. **Mock Metrics Stream**: Draw functions in the TUI module must render from a state object that is decoupled from active channels and running compressor threads.
2. **TestBackend Snapshots**: In unit/layout tests, initialize the TUI state with predefined mock metrics (e.g., 50% split progress, 150ms throttle delay, level 7, focus set to Level Slider).
3. **No File Operations**: Running snapshot tests must not generate output files or read input files.

---

## 🏛️ Technical Architecture (Morpheus)

### 1. Module Layout

```text
src/
├── main.rs         # Parses CLI arguments, selects interface mode, runs app entry point.
├── lib.rs          # Declares library modules and exports public API.
├── compressor.rs   # Compression algorithms (gzip, bzip2, xz) with progress hooks.
├── pipeline.rs     # The core CompressionPipeline and PipelineController structures.
├── split_mode.rs   # Concurrent block compression for static files.
├── stream_mode.rs  # Pipelined concurrent chunk compression for streams.
└── tui/
    ├── mod.rs      # Main thread event loop, event pooling, state tracker.
    ├── widgets.rs  # Vertical sliders and custom LCARS dashboards.
    └── theme.rs    # LCARS retro palette constants.
```

### 2. De-coupled Communication and Data Flow

Rather than updating shared state directly, the compression pipeline publishes `ProgressEvent` messages to a thread-safe channel, which are processed sequentially by the TUI event loop on the main thread:

```text
+-------------------------------------------------------------------------+
|                           Compression Library                           |
|                                                                         |
|  +--------------------+                                                 |
|  | CompressionPipeline| -- Spawns --> [ Reader Thread ]                 |
|  |                    |                [ Writer Thread ]                |
|  |                    |                [ Rayon Worker Pool ]            |
|  +--------------------+                         |                       |
|           ^                                     v                       |
|           | Updates Control Flags       Sends Progress Metrics          |
|           | (Atomic Level / Throttle)   (Bytes read/written, status)    |
|           |                                     |                       |
+-----------|-------------------------------------|-----------------------+
            |                                     |
            | controller.update_*()               | rx.recv() / try_recv()
            |                                     v
+-----------|-------------------------------------------------------------+
|           |                         TUI Frontend                        |
|  +--------------------+                                                 |
|  | PipelineController |                                                 |
|  +--------------------+                                                 |
|           ^                                                             |
|           | Key/Mouse Adjustments                                       |
|  +--------------------+         Update State      +------------------+  |
|  |   Main Event Loop  | ------------------------> |     TuiState     |  |
|  | (crossterm poll)   |                           +------------------+  |
|  +--------------------+                                     |           |
|           ^                                                 | Renders   |
|           | crossterm inputs                                v           |
|  +--------------------+                           +------------------+  |
|  |   Keyboard/Mouse   |                           |    draw_tui()    |  |
|  +--------------------+                           +------------------+  |
+-------------------------------------------------------------------------+
```

### 3. Public API Signatures

```rust
// In src/pipeline.rs:

pub enum ProgressEvent {
    SplitProgress { stripe_id: usize, bytes_processed: usize, bytes_written: usize, total_bytes: usize },
    StreamProgress { bytes_read: usize, bytes_written: usize, queue_depth: usize },
    WorkerStatus { worker_id: usize, status: &'static str, current_chunk: Option<u64> },
    AvgCompressionTime(std::time::Duration),
    Error(ZipError),
    Complete,
}

#[derive(Clone)]
pub struct PipelineController {
    is_paused: Arc<AtomicBool>,
    throttle_delay_ms: Arc<AtomicU64>,
    compression_level: Arc<AtomicU32>,
    is_aborted: Arc<AtomicBool>,
}

impl PipelineController {
    pub fn update_level(&self, level: u32) {
        self.compression_level.store(level, Ordering::Relaxed);
    }

    pub fn update_throttle(&self, delay_ms: u64) {
        self.throttle_delay_ms.store(delay_ms, Ordering::Relaxed);
    }

    pub fn pause(&self) {
        self.is_paused.store(true, Ordering::Relaxed);
    }

    pub fn resume(&self) {
        self.is_paused.store(false, Ordering::Relaxed);
    }

    pub fn abort(&self) {
        self.is_aborted.store(true, Ordering::Relaxed);
    }
}

pub struct CompressionPipeline {
    compressor: Arc<Box<dyn Compressor + Send + Sync>>,
    num_threads: usize,
}

impl CompressionPipeline {
    pub fn new(compressor: Arc<Box<dyn Compressor + Send + Sync>>, num_threads: usize) -> Self {
        Self { compressor, num_threads }
    }

    pub fn run(
        &self,
        input_source: InputSource,
        output_dest: OutputDestination,
    ) -> (PipelineController, std::sync::mpsc::Receiver<ProgressEvent>) {
        // Initializes controller and channel, spawns compression thread, and returns handles
        todo!()
    }
}
```

### 4. Interactive Sliders and Focus Cycling

- Visual vertical sliders are rendered using custom block drawing widgets where the current value determines the filled/empty proportions of the column blocks.
- **Tab Focus Cycle State**:
  ```rust
  #[derive(Copy, Clone, PartialEq, Eq, Debug)]
  pub enum FocusedWidget {
      None,
      CompressionLevelSlider,
      ThrottleDelaySlider,
  }
  ```
- **Navigation logic mapping**:
  - Tab: `FocusedWidget` transitions: `None` $\rightarrow$ `CompressionLevelSlider` $\rightarrow$ `ThrottleDelaySlider` $\rightarrow$ `None`.
  - Arrow Up/Down: updates target parameter inside `TuiState` and propagates via `PipelineController`.
- **Mouse Event integration**:
  ```rust
  if let Event::Mouse(mouse_event) = event::read().unwrap() {
      if mouse_event.kind == MouseEventKind::Down(MouseButton::Left) || mouse_event.kind == MouseEventKind::Drag(MouseButton::Left) {
          // Check if coordinate intersects slider rectangles
          if let Some(val) = check_slider_intersection(mouse_event.column, mouse_event.row, slider_rect) {
              controller.update_slider_value(val);
          }
      }
  }
  ```

### 5. Snapshot Testing Architecture

Instead of launching actual file I/O operations, the snapshot tests will pass mock statistics to a decoupled `draw_tui` function using `TestBackend`:

```rust
#[test]
fn test_tui_layout_split_mode_snapshot() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    
    let mut mock_state = TuiState::new_split(4, 1024 * 1024, 6);
    mock_state.stripes[0].bytes_processed = 256 * 1024;
    mock_state.stripes[0].total_bytes = 256 * 1024;
    mock_state.stripes[1].bytes_processed = 128 * 1024;
    mock_state.stripes[1].total_bytes = 256 * 1024;
    mock_state.focused_widget = FocusedWidget::CompressionLevelSlider;
    
    terminal.draw(|f| {
        draw_tui_layout(f, &mock_state);
    }).unwrap();
    
    let buffer = terminal.backend().buffer();
    assert_snapshot_buffer(buffer);
}
```
