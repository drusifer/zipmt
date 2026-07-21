# Architecture: Rust TUI Decomposition and Application Boundaries

**Classification:** Non-functional enhancement. No feature, UX, CLI, or archive
behavior change is intended or authorized.

## Sequence

1. Extract dashboard panel renderers.
2. Extract chart composition.
3. Separate terminal lifecycle and event-loop roles.
4. Split keyboard command families.
5. Extract application startup services.

Rendering precedes runtime changes so snapshots provide a stable visual gate.
Runtime precedes command dispatch so input routing has a narrow host.
Application startup is last because it composes the completed runtime.

## Rendering boundaries

- `FrameContext`: elapsed/status/palette and immutable `TuiState`.
- `DashboardAreas`: header, work, chart/process, logs, and footer rectangles.
- Standalone functions render one panel and receive only their area/context.
- Chart preparation returns owned view data; chart rendering consumes it.

## Runtime boundaries

- `TerminalSession` owns raw mode, alternate screen, mouse capture, and restore.
- `RuntimeTick` drains progress and samples state.
- Event dispatch returns a typed runtime action.
- Pipeline joining and error translation remain outside rendering.

## Input boundaries

- Global commands are evaluated first.
- Navigation, focused-control adjustment, and log navigation are separate
  families.
- Mouse handling uses the same state commands where practical.

## Application boundaries

- `resolve_compressor` returns `Result<Arc<dyn Compressor>, CliError>`.
- `install_signal_handler` owns registration only.
- `exit_code_for_error` is pure.
- `main` parses, resolves, installs, runs, reports, and exits.

## Verification

- Characterization tests precede each extraction.
- Snapshots gate every rendering phase.
- Terminal restoration receives success/error/abort coverage.
- CLI help, diagnostics, cleanup, and exit codes receive binary tests.
- Deterministic quality gates run per phase; benchmark runs only if hot-adjacent
  code changes and at epic close.
