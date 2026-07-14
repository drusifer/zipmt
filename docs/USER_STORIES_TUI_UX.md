# User Stories & Acceptance Criteria: TUI UX Upgrade

## Epic: Retro Star Trek LCARS Theme & Diagnostic Metrics

### Story 1: Star Trek LCARS Retro Styling
- **As a** terminal user compressing files or streams
- **I want to** view a colorful retro Star Trek LCARS console display
- **So that** the TUI interface looks premium, diagnostic, and retro.

#### Acceptance Criteria
1. Displays LCARS-style headers, borders, and rounded bracket blocks using ANSI lines/chars (e.g. `[=== LCARS SYSTEM CONTROL ===]`).
2. Utilizes LCARS palette colors:
   - Orange: `\x1B[38;5;208m`
   - Purple/Lavender: `\x1B[38;5;147m`
   - Cyan: `\x1B[38;5;117m`
   - Yellow: `\x1B[38;5;220m`
3. Layout segments details into:
   - System Diagnostics (overall status)
   - Sector Progress (stripe sectors in split mode)
   - Transporter Buffer (queues/channels in stream mode)

### Story 2: Dynamic ETA (Known Input Size)
- **As a** user compressing a file with known size
- **I want to** see a calculated ETA timer (e.g., `ETA: 12s`)
- **So that** I know exactly when the compression task will finish.

#### Acceptance Criteria
1. If input size is known (Split Mode / file compression), calculate average bytes-per-second (`total_in / elapsed`).
2. Display ETA in seconds (`remaining_bytes / bytes_per_second`).
3. Display `ETA: --` or `Estimating...` if processing has just started.

### Story 3: Projection Forecast (Unknown Input Size / Stream Mode)
- **As a** user compressing a stream from standard input (unknown size)
- **I want to** see data processing projections for 1-minute, 5-minute, and 10-minute intervals at current speeds
- **So that** I can forecast processing capacity.

#### Acceptance Criteria
1. If input size is unknown (Stream Mode / stdin), display projection rates.
2. Based on current throughput, calculate predicted processed volume:
   - `1m Forecast: <MB>`
   - `5m Forecast: <MB>`
   - `10m Forecast: <MB>`
