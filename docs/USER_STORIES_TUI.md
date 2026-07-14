# User Stories & Acceptance Criteria: TUI Progress Visualizer

## Epic: TUI queue progress visualization

### Story 1: TUI option activation
- **As a** user compressing files or streams
- **I want to** pass a `--tui` / `-T` flag to display a real-time terminal UI progress display
- **So that** I can visually inspect queue states, compression rates, and stripe progress.

#### Acceptance Criteria
1. CLI accepts `--tui` / `-T` flag.
2. If `--tui` is provided, standard console output logs (verbose or normal) are suppressed and replaced by the TUI rendering terminal interface.
3. If stdout is redirected (piped or stdout mode), `--tui` is disabled or prints its visual outputs strictly to stderr.

### Story 2: Split Mode Multi-Stripe visualization
- **As a** user compressing a file in Split Mode
- **I want to** see a list of all stripes/splits being processed in parallel
- **So that** I can track each split's bytes read vs. bytes written, and its individual compression rate.

#### Acceptance Criteria
1. Renders a vertical list representing each split/stripe.
2. For each split, shows progress bar (bytes read vs. total bytes of split).
3. Shows input bytes read, output bytes written, and instantaneous compression ratio (e.g. `1.8x` or `-45%`).

### Story 3: Stream Mode queue/pool visualization
- **As a** user compressing a stream in Stream Mode
- **I want to** see the status of the job/result queues and current I/O rates
- **So that** I can monitor backpressure throttling and throughput performance.

#### Acceptance Criteria
1. Displays current queue usage (e.g., `Queue: 3/8 blocks occupied`).
2. Displays cumulative input read bytes, output written bytes, and overall compression ratio.
3. Displays instantaneous throughput / I/O speed (e.g., `MB/sec`).
4. Updates display at a regular interval (e.g., 100ms or 200ms).
