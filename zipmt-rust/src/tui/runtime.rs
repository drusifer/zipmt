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

pub(super) enum RuntimeAction {
    Continue,
    Exit,
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
    match code {
        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
            if state.lock().unwrap().is_complete {
                RuntimeAction::Exit
            } else {
                RuntimeAction::Abort
            }
        }
        KeyCode::Enter if state.lock().unwrap().is_complete => RuntimeAction::Exit,
        KeyCode::Char('p') | KeyCode::Char('P') => {
            let current = controller.is_paused();
            if current {
                controller.resume();
            } else {
                controller.pause();
            }
            state.lock().unwrap().is_paused = !current;
            RuntimeAction::Continue
        }
        KeyCode::Char('i') | KeyCode::Char('I') => {
            state.lock().unwrap().toggle_io_chart_mode();
            RuntimeAction::Continue
        }
        KeyCode::Char('-') => {
            update_throttle(state, controller, true);
            RuntimeAction::Continue
        }
        KeyCode::Char('+') | KeyCode::Char('=') => {
            update_throttle(state, controller, false);
            RuntimeAction::Continue
        }
        KeyCode::Char('[') => {
            update_level(state, controller, false);
            RuntimeAction::Continue
        }
        KeyCode::Char(']') => {
            update_level(state, controller, true);
            RuntimeAction::Continue
        }
        KeyCode::Tab => {
            let mut state = state.lock().unwrap();
            state.focused_widget = super::next_focus_for_mode(state.mode, state.focused_widget);
            RuntimeAction::Continue
        }
        KeyCode::PageUp => {
            page_work_list(state, false);
            RuntimeAction::Continue
        }
        KeyCode::PageDown => {
            page_work_list(state, true);
            RuntimeAction::Continue
        }
        KeyCode::Home => {
            crate::LOG_SCROLL_OFFSET.store(usize::MAX, Ordering::Relaxed);
            RuntimeAction::Continue
        }
        KeyCode::End => {
            crate::LOG_SCROLL_OFFSET.store(0, Ordering::Relaxed);
            RuntimeAction::Continue
        }
        KeyCode::Up => {
            adjust_focused(state, controller, true);
            RuntimeAction::Continue
        }
        KeyCode::Down => {
            adjust_focused(state, controller, false);
            RuntimeAction::Continue
        }
        _ => RuntimeAction::Continue,
    }
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
    controller.update_level(next);
    state.lock().unwrap().level = next;
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
    controller.update_throttle(next);
    state.lock().unwrap().throttle_delay_ms = next;
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
        controller.update_level(level);
        let mut state = state.lock().unwrap();
        state.level = level;
        state.focused_widget = FocusedWidget::CompressionLevelSlider;
    } else if (controls_left + 13..=controls_left + 25).contains(&col) {
        let fraction = super::slider_fraction(slider_row, control_height);
        let delay = ((1.0 - fraction) * 500.0).round() as u64;
        controller.update_throttle(delay);
        let mut state = state.lock().unwrap();
        state.throttle_delay_ms = delay;
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

pub fn run_tui_on_main_thread(
    state: Arc<Mutex<TuiState>>,
    rx: std::sync::mpsc::Receiver<crate::pipeline::ProgressEvent>,
    controller: crate::pipeline::PipelineController,
    comp_handle: std::thread::JoinHandle<Result<(), crate::compressor::ZipError>>,
) -> Result<(), crate::compressor::ZipError> {
    super::run_tui_on_main_thread_impl(state, rx, controller, comp_handle)
}
