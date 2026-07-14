# User Stories & Acceptance Criteria: Full-Screen Star Trek LCARS TUI

## Epic: Full-Screen Interactive Diagnostics Console & Throttling

### Story 1: Alternate Screen alternate buffer drawing
- **As a** user compressing files or streams
- **I want to** view a full-screen dashboard in the alternate terminal screen buffer
- **So that** it does not clutter my terminal command scrollback on exit.

#### Acceptance Criteria
1. TUI enters alternate buffer (`\x1B[?1049h`) and hides cursor at startup, and restores standard buffer (`\x1B[?1049l`) and cursor on exit.
2. Draws boxes using standard unicode box-drawing characters (`┌`, `─`, `│`, etc.).

### Story 2: Pretty Progress Bars
- **As a** user monitoring sector states
- **I want** progress bars to use premium solid blocks (`█`) and shaded empty blocks (`░`)
- **So that** the visual states look premium and modern (btop style).

#### Acceptance Criteria
1. Replaces standard `[` / `=` progress bars with filled `█` blocks and empty `░` backgrounds.

### Story 3: Ingestion Speed Timeline Graph
- **As a** user monitoring throughput stability
- **I want to** see a live scrolling historical chart of MB/s rates over time
- **So that** I can visually inspect compression performance stability.

#### Acceptance Criteria
1. Renders a scrolling column chart inside a dedicated "SPEED HISTORY" box.
2. Uses block-height characters (` `, `▂`, `▃`, `▄`, `▅`, `▆`, `▇`, `█`) to represent throughput scale.

### Story 4: Keyboard Speed Throttling Control
- **As a** user compressing a massive archive
- **I want to** hit keypresses (`+` / `-` / `p` / `q`) to dynamically adjust compression rates
- **So that** I can free up CPU cores for other applications or cleanly abort.

#### Acceptance Criteria
1. TUI listens to keyboard events in real-time.
2. Pressing `-` (Slow Down) increases a shared atomic sleep throttling delay by `50ms` (capping at `500ms`), slowing down throughput.
3. Pressing `+` or `=` (Speed Up) decreases the throttling delay by `50ms` (down to `0ms`), restoring full speed.
4. Pressing `P` (Pause) parks all worker threads until `P` is pressed again to resume.
5. Pressing `Q` or `Esc` triggers immediate safe abort, triggers output file removal, and returns exit code `2`.
