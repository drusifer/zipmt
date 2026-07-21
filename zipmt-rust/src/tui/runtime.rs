use std::io::Write;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use crossterm::event::{Event, KeyCode, MouseEvent, MouseEventKind};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use super::TuiState;
use super::{FocusedWidget, ModeState};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum RuntimeAction {
    Continue,
    Exit,
    Abort,
}

pub(super) enum SessionOutcome {
    Complete,
    Abort,
}

pub(super) fn handle_event(
    event: Event,
    state: &Arc<Mutex<TuiState>>,
    controller: &crate::pipeline::PipelineController,
) -> RuntimeAction {
    match event {
        Event::Key(key) => handle_key(key.code, state, controller),
        Event::Resize(cols, rows) => {
            let mut state = state.lock().unwrap();
            state.terminal_cols = cols;
            state.terminal_rows = rows;
            RuntimeAction::Continue
        }
        Event::Mouse(mouse) => {
            handle_mouse(mouse, state, controller);
            RuntimeAction::Continue
        }
        _ => RuntimeAction::Continue,
    }
}

fn handle_key(
    code: KeyCode,
    state: &Arc<Mutex<TuiState>>,
    controller: &crate::pipeline::PipelineController,
) -> RuntimeAction {
    if let Some(action) = global_key_action(code, state, controller) {
        return action;
    }
    if handle_work_list_key(code, state) {
        return RuntimeAction::Continue;
    }
    if handle_control_key(code, state, controller) {
        return RuntimeAction::Continue;
    }
    handle_log_key(code);
    RuntimeAction::Continue
}

fn global_key_action(
    code: KeyCode,
    state: &Arc<Mutex<TuiState>>,
    controller: &crate::pipeline::PipelineController,
) -> Option<RuntimeAction> {
    match code {
        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
            Some(if state.lock().unwrap().is_complete {
                RuntimeAction::Exit
            } else {
                RuntimeAction::Abort
            })
        }
        KeyCode::Enter if state.lock().unwrap().is_complete => Some(RuntimeAction::Exit),
        KeyCode::Char('p') | KeyCode::Char('P') => {
            let current = controller.is_paused();
            if current {
                controller.resume();
            } else {
                controller.pause();
            }
            state.lock().unwrap().is_paused = !current;
            Some(RuntimeAction::Continue)
        }
        KeyCode::Char('i') | KeyCode::Char('I') => {
            state.lock().unwrap().toggle_io_chart_mode();
            Some(RuntimeAction::Continue)
        }
        _ => None,
    }
}

fn handle_work_list_key(code: KeyCode, state: &Arc<Mutex<TuiState>>) -> bool {
    match code {
        KeyCode::PageUp => page_work_list(state, false),
        KeyCode::PageDown => page_work_list(state, true),
        _ => return false,
    }
    true
}

fn handle_control_key(
    code: KeyCode,
    state: &Arc<Mutex<TuiState>>,
    controller: &crate::pipeline::PipelineController,
) -> bool {
    match code {
        KeyCode::Char('-') => {
            update_throttle(state, controller, true);
        }
        KeyCode::Char('+') | KeyCode::Char('=') => {
            update_throttle(state, controller, false);
        }
        KeyCode::Char('[') => {
            update_level(state, controller, false);
        }
        KeyCode::Char(']') => {
            update_level(state, controller, true);
        }
        KeyCode::Tab => {
            let mut state = state.lock().unwrap();
            state.focused_widget = super::next_focus_for_mode(state.mode, state.focused_widget);
        }
        KeyCode::Up => {
            adjust_focused(state, controller, true);
        }
        KeyCode::Down => {
            adjust_focused(state, controller, false);
        }
        _ => return false,
    }
    true
}

fn handle_log_key(code: KeyCode) -> bool {
    let offset = match code {
        KeyCode::Home => usize::MAX,
        KeyCode::End => 0,
        _ => return false,
    };
    crate::LOG_SCROLL_OFFSET.store(offset, Ordering::Relaxed);
    true
}

fn update_level(
    state: &Arc<Mutex<TuiState>>,
    controller: &crate::pipeline::PipelineController,
    increase: bool,
) {
    if state.lock().unwrap().mode == ModeState::Split {
        return;
    }
    let current = controller.compression_level();
    let next = if increase {
        (current + 1).min(9)
    } else {
        current.saturating_sub(1).max(1)
    };
    set_level(state, controller, next);
}

fn set_level(
    state: &Arc<Mutex<TuiState>>,
    controller: &crate::pipeline::PipelineController,
    level: u32,
) {
    controller.update_level(level);
    state.lock().unwrap().level = level;
}

fn update_throttle(
    state: &Arc<Mutex<TuiState>>,
    controller: &crate::pipeline::PipelineController,
    increase: bool,
) {
    let current = controller.throttle_delay_ms();
    let next = if increase {
        current.saturating_add(50).min(500)
    } else {
        current.saturating_sub(50)
    };
    set_throttle(state, controller, next);
}

fn set_throttle(
    state: &Arc<Mutex<TuiState>>,
    controller: &crate::pipeline::PipelineController,
    delay: u64,
) {
    controller.update_throttle(delay);
    state.lock().unwrap().throttle_delay_ms = delay;
}

pub(super) fn page_work_list(state: &Arc<Mutex<TuiState>>, forward: bool) {
    let mut state = state.lock().unwrap();
    let page = match state.mode {
        ModeState::Split => super::split_page_size(state.terminal_rows),
        ModeState::Stream => super::worker_page_size(state.terminal_rows),
    };
    let item_count = match state.mode {
        ModeState::Split => state.stripes.len(),
        ModeState::Stream => state.workers.len(),
    };
    let offset = match state.mode {
        ModeState::Split => &mut state.split_sector_offset,
        ModeState::Stream => &mut state.worker_offset,
    };
    if forward {
        let max_offset = item_count.saturating_sub(page);
        *offset = (*offset + page).min(max_offset);
    } else {
        *offset = offset.saturating_sub(page);
    }
}

fn adjust_focused(
    state: &Arc<Mutex<TuiState>>,
    controller: &crate::pipeline::PipelineController,
    increase: bool,
) {
    let focused = state.lock().unwrap().focused_widget;
    match focused {
        FocusedWidget::CompressionLevelSlider => update_level(state, controller, increase),
        FocusedWidget::ThrottleDelaySlider => update_throttle(state, controller, increase),
        FocusedWidget::ChunkSizeSlider => update_chunk_size(state, controller, increase),
        FocusedWidget::WorkerCountSlider => update_worker_count(state, controller, increase),
        FocusedWidget::None => {
            let offset = crate::LOG_SCROLL_OFFSET.load(Ordering::Relaxed);
            let next = if increase {
                offset.saturating_add(1)
            } else {
                offset.saturating_sub(1)
            };
            crate::LOG_SCROLL_OFFSET.store(next, Ordering::Relaxed);
        }
    }
}

fn update_chunk_size(
    state: &Arc<Mutex<TuiState>>,
    controller: &crate::pipeline::PipelineController,
    increase: bool,
) {
    let current = controller.chunk_size();
    let next = if increase {
        current
            .saturating_mul(2)
            .min(crate::pipeline::MAX_CHUNK_SIZE)
    } else {
        (current / 2).max(crate::pipeline::MIN_CHUNK_SIZE)
    };
    if controller.update_chunk_size(next) {
        state.lock().unwrap().chunk_size = next;
    }
}

fn update_worker_count(
    state: &Arc<Mutex<TuiState>>,
    controller: &crate::pipeline::PipelineController,
    increase: bool,
) {
    let current = controller.active_workers();
    let next = if increase {
        current.saturating_add(1).min(controller.max_workers())
    } else {
        current.saturating_sub(1).max(1)
    };
    if controller.update_active_workers(next) {
        state.lock().unwrap().active_workers = next;
    }
}

fn handle_mouse(
    mouse: MouseEvent,
    state: &Arc<Mutex<TuiState>>,
    controller: &crate::pipeline::PipelineController,
) {
    let (cols, rows) = {
        let state = state.lock().unwrap();
        (state.terminal_cols, state.terminal_rows)
    };
    if update_work_list_scroll(mouse, state)
        || update_log_scroll(mouse, rows)
        || cols < 80
        || rows < 22
        || !is_left_action(mouse.kind)
    {
        return;
    }
    update_control_from_mouse(mouse, cols, rows, state, controller);
}

pub(super) fn update_work_list_scroll(mouse: MouseEvent, state: &Arc<Mutex<TuiState>>) -> bool {
    if !matches!(
        mouse.kind,
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
    ) {
        return false;
    }
    let (cols, rows, mode) = {
        let state = state.lock().unwrap();
        (state.terminal_cols, state.terminal_rows, state.mode)
    };
    if cols < 80 || rows < 22 {
        return false;
    }
    let body_height = 11 + rows.saturating_sub(22) / 2;
    let left_width = cols.saturating_mul(super::body_layout_profile(mode).left_percent) / 100;
    if mouse.row == 0 || mouse.row > body_height || mouse.column >= left_width {
        return false;
    }
    page_work_list(state, mouse.kind == MouseEventKind::ScrollDown);
    true
}

fn update_log_scroll(mouse: MouseEvent, rows: u16) -> bool {
    let footer_height = super::footer_height_for_rows(rows);
    let body_height = 11 + rows.saturating_sub(22) / 2;
    let in_logs = mouse.row > body_height && mouse.row < rows.saturating_sub(footer_height);
    if !in_logs {
        return false;
    }
    let offset = crate::LOG_SCROLL_OFFSET.load(Ordering::Relaxed);
    match mouse.kind {
        MouseEventKind::ScrollUp => {
            crate::LOG_SCROLL_OFFSET.store(offset.saturating_add(3), Ordering::Relaxed);
            true
        }
        MouseEventKind::ScrollDown => {
            crate::LOG_SCROLL_OFFSET.store(offset.saturating_sub(3), Ordering::Relaxed);
            true
        }
        _ => false,
    }
}

fn is_left_action(kind: MouseEventKind) -> bool {
    kind == MouseEventKind::Down(crossterm::event::MouseButton::Left)
        || matches!(
            kind,
            MouseEventKind::Drag(crossterm::event::MouseButton::Left)
        )
}

fn update_control_from_mouse(
    mouse: MouseEvent,
    cols: u16,
    rows: u16,
    state: &Arc<Mutex<TuiState>>,
    controller: &crate::pipeline::PipelineController,
) {
    let footer_height = super::footer_height_for_rows(rows) as usize;
    let controls_left = (cols as usize).saturating_sub(52);
    let content_top = (rows as usize).saturating_sub(footer_height) + 1;
    let control_height = footer_height.saturating_sub(2);
    let row = mouse.row as usize;
    if row < content_top || row >= content_top + control_height {
        return;
    }

    let col = mouse.column as usize;
    let mode = state.lock().unwrap().mode;
    let slider_row = row - content_top;
    if mode == ModeState::Stream && (controls_left..=controls_left + 12).contains(&col) {
        let fraction = super::slider_fraction(slider_row, control_height);
        let level = (9.0 - fraction * 8.0).round() as u32;
        set_level(state, controller, level);
        let mut state = state.lock().unwrap();
        state.focused_widget = FocusedWidget::CompressionLevelSlider;
    } else if (controls_left + 13..=controls_left + 25).contains(&col) {
        let fraction = super::slider_fraction(slider_row, control_height);
        let delay = ((1.0 - fraction) * 500.0).round() as u64;
        set_throttle(state, controller, delay);
        let mut state = state.lock().unwrap();
        state.focused_widget = FocusedWidget::ThrottleDelaySlider;
    } else if (controls_left + 26..=controls_left + 38).contains(&col) {
        let chunk_size = super::chunk_size_from_slider_row(slider_row, control_height);
        controller.update_chunk_size(chunk_size);
        let mut state = state.lock().unwrap();
        state.chunk_size = chunk_size;
        state.focused_widget = FocusedWidget::ChunkSizeSlider;
    } else if mode == ModeState::Stream && (controls_left + 39..=controls_left + 51).contains(&col)
    {
        let workers =
            super::workers_from_slider_row(slider_row, control_height, controller.max_workers());
        controller.update_active_workers(workers);
        let mut state = state.lock().unwrap();
        state.active_workers = workers;
        state.focused_widget = FocusedWidget::WorkerCountSlider;
    }
}

pub(super) struct TerminalGuard {
    pub(super) terminal: Terminal<CrosstermBackend<std::io::Stderr>>,
}

impl TerminalGuard {
    pub(super) fn new() -> Result<Self, std::io::Error> {
        let _ = enable_raw_mode();
        let mut stderr = std::io::stderr();
        let _ = crossterm::queue!(
            stderr,
            EnterAlternateScreen,
            crossterm::cursor::Hide,
            crossterm::event::EnableMouseCapture
        );
        let _ = stderr.flush();
        let backend = CrosstermBackend::new(stderr);
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let mut stderr = std::io::stderr();
        let _ = crossterm::queue!(
            stderr,
            crossterm::cursor::Show,
            LeaveAlternateScreen,
            crossterm::event::DisableMouseCapture
        );
        let _ = stderr.flush();
    }
}

fn drain_progress(
    receiver: &std::sync::mpsc::Receiver<crate::pipeline::ProgressEvent>,
    state: &Arc<Mutex<TuiState>>,
) {
    while let Ok(event) = receiver.try_recv() {
        state.lock().unwrap().apply_progress_event(event);
    }
}

fn poll_input(
    tick_rate: std::time::Duration,
    state: &Arc<Mutex<TuiState>>,
    controller: &crate::pipeline::PipelineController,
) -> RuntimeAction {
    if !crossterm::event::poll(tick_rate).unwrap_or(false) {
        return RuntimeAction::Continue;
    }
    let mut action = RuntimeAction::Continue;
    while crossterm::event::poll(std::time::Duration::ZERO).unwrap_or(false) {
        match handle_event(crossterm::event::read().unwrap(), state, controller) {
            RuntimeAction::Continue => {}
            RuntimeAction::Exit => action = RuntimeAction::Exit,
            RuntimeAction::Abort => return RuntimeAction::Abort,
        }
    }
    action
}

fn render_tick(
    guard: &mut TerminalGuard,
    state: &Arc<Mutex<TuiState>>,
    tick_rate: std::time::Duration,
) {
    let mut state = state.lock().unwrap();
    let now = std::time::Instant::now();
    let elapsed = now.duration_since(state.last_speed_update);
    if !state.is_complete && elapsed >= tick_rate {
        state.sample_system_metrics(elapsed);
        state.last_speed_update = now;
    }
    if !state.is_complete {
        state.sample_io_bucket(now);
    }
    let _ = super::draw_tui(&mut guard.terminal, &state);
}

fn join_compression(
    handle: std::thread::JoinHandle<Result<(), crate::compressor::ZipError>>,
) -> Result<(), crate::compressor::ZipError> {
    handle.join().unwrap_or_else(|_| {
        Err(crate::compressor::ZipError::Io(std::io::Error::other(
            "Compression thread panicked",
        )))
    })
}

pub(super) fn drive_session(
    guard: &mut TerminalGuard,
    state: &Arc<Mutex<TuiState>>,
    receiver: &std::sync::mpsc::Receiver<crate::pipeline::ProgressEvent>,
    controller: &crate::pipeline::PipelineController,
    handle: std::thread::JoinHandle<Result<(), crate::compressor::ZipError>>,
) -> Result<SessionOutcome, crate::compressor::ZipError> {
    let tick_rate = std::time::Duration::from_millis(100);
    let hold_on_complete = std::env::var("ZIPMT_FORCE_TUI").ok().as_deref() != Some("1");
    loop {
        drain_progress(receiver, state);
        match poll_input(tick_rate, state, controller) {
            RuntimeAction::Continue => {}
            RuntimeAction::Exit => break,
            RuntimeAction::Abort => {
                controller.abort();
                crate::cleanup_output_file();
                return Ok(SessionOutcome::Abort);
            }
        }
        render_tick(guard, state, tick_rate);
        if !hold_on_complete && state.lock().unwrap().is_complete {
            break;
        }
        if handle.is_finished() {
            drain_progress(receiver, state);
            if !state.lock().unwrap().is_complete {
                break;
            }
        }
    }
    join_compression(handle).map(|()| SessionOutcome::Complete)
}

pub fn run_tui_on_main_thread(
    state: Arc<Mutex<TuiState>>,
    rx: std::sync::mpsc::Receiver<crate::pipeline::ProgressEvent>,
    controller: crate::pipeline::PipelineController,
    comp_handle: std::thread::JoinHandle<Result<(), crate::compressor::ZipError>>,
) -> Result<(), crate::compressor::ZipError> {
    super::run_tui_on_main_thread_impl(state, rx, controller, comp_handle)
}
