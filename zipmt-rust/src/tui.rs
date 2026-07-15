use std::io::Write;
use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering;
use std::time::Instant;

use crossterm::event::{self, Event, KeyCode};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use ratatui::style::{Color, Style, Modifier};
use ratatui::text::{Line, Span};
use ratatui::layout::{Rect, Layout, Constraint, Direction};
use ratatui::widgets::{Paragraph, Clear};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TuiMode {
    Split,
    Stream,
}

pub struct StripeProgress {
    pub id: usize,
    pub bytes_processed: usize,
    pub total_bytes: usize,
    pub bytes_written: usize,
}

#[derive(Clone, Debug)]
pub struct WorkerState {
    pub id: usize,
    pub status: &'static str, // "IDLE", "BUSY", "HOLD"
    pub current_chunk: Option<u64>,
}

pub struct TuiState {
    pub mode: TuiMode,
    pub start_time: Instant,
    pub is_complete: bool,
    pub total_input_size: usize,
    // Split mode progress
    pub stripes: Vec<StripeProgress>,
    // Stream mode progress
    pub bytes_read: usize,
    pub bytes_written: usize,
    pub queue_depth: usize,
    pub queue_capacity: usize,
    // Rolling speed history (max 35 values)
    pub speed_history: Vec<f64>,
    // Terminal dimensions
    pub terminal_cols: u16,
    pub terminal_rows: u16,
    // Rolling instant speed tracking
    pub prev_total_in: usize,
    pub last_speed_update: Instant,
    // Queue / Worker / Gaps tracking
    pub input_queue: Vec<u64>,
    pub workers: Vec<WorkerState>,
    pub output_buffer: Vec<u64>,
    pub next_expected_seq: u64,
    // Compression level
    pub level: u32,
}

impl TuiState {
    pub fn new_split(total_stripes: usize, total_input_size: usize, level: u32) -> Self {
        let mut stripes = Vec::new();
        for id in 0..total_stripes {
            stripes.push(StripeProgress {
                id,
                bytes_processed: 0,
                total_bytes: 0,
                bytes_written: 0,
            });
        }
        TuiState {
            mode: TuiMode::Split,
            start_time: Instant::now(),
            is_complete: false,
            total_input_size,
            stripes,
            bytes_read: 0,
            bytes_written: 0,
            queue_depth: 0,
            queue_capacity: 0,
            speed_history: Vec::new(),
            terminal_cols: 80,
            terminal_rows: 24,
            prev_total_in: 0,
            last_speed_update: Instant::now(),
            input_queue: Vec::new(),
            workers: Vec::new(),
            output_buffer: Vec::new(),
            next_expected_seq: 0,
            level,
        }
    }

    pub fn new_stream(queue_capacity: usize, total_input_size: usize, num_workers: usize, level: u32) -> Self {
        let mut workers = Vec::new();
        for id in 0..num_workers {
            workers.push(WorkerState {
                id,
                status: "IDLE",
                current_chunk: None,
            });
        }
        TuiState {
            mode: TuiMode::Stream,
            start_time: Instant::now(),
            is_complete: false,
            total_input_size,
            stripes: Vec::new(),
            bytes_read: 0,
            bytes_written: 0,
            queue_depth: 0,
            queue_capacity,
            speed_history: Vec::new(),
            terminal_cols: 80,
            terminal_rows: 24,
            prev_total_in: 0,
            last_speed_update: Instant::now(),
            input_queue: Vec::new(),
            workers,
            output_buffer: Vec::new(),
            next_expected_seq: 0,
            level,
        }
    }
}

struct TerminalGuard {
    pub terminal: Terminal<CrosstermBackend<std::io::Stderr>>,
}
impl TerminalGuard {
    pub fn new() -> Result<Self, std::io::Error> {
        let _ = enable_raw_mode();
        let mut stderr = std::io::stderr();
        let _ = crossterm::queue!(stderr, EnterAlternateScreen, crossterm::cursor::Hide);
        let _ = stderr.flush();
        let backend = CrosstermBackend::new(stderr);
        let terminal = Terminal::new(backend)?;
        Ok(TerminalGuard { terminal })
    }
}
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let mut stderr = std::io::stderr();
        let _ = crossterm::queue!(stderr, crossterm::cursor::Show, LeaveAlternateScreen);
        let _ = stderr.flush();
    }
}

pub fn query_initial_size() -> (u16, u16) {
    if let (Ok(cols_str), Ok(rows_str)) = (std::env::var("COLUMNS"), std::env::var("LINES")) {
        if let (Ok(cols), Ok(rows)) = (cols_str.parse::<u16>(), rows_str.parse::<u16>()) {
            return (cols, rows);
        }
    }

    // Try stty size with redirected TTY or stderr/proc streams
    for path in &["/dev/tty", "/dev/stderr", "/proc/self/fd/2"] {
        if let Ok(file) = std::fs::File::open(path) {
            if let Ok(output) = std::process::Command::new("stty")
                .arg("size")
                .stdin(file)
                .output()
            {
                let out_str = String::from_utf8_lossy(&output.stdout);
                let parts: Vec<&str> = out_str.split_whitespace().collect();
                if parts.len() == 2 {
                    if let (Ok(rows), Ok(cols)) = (parts[0].parse::<u16>(), parts[1].parse::<u16>()) {
                        if cols > 0 && rows > 0 {
                            return (cols, rows);
                        }
                    }
                }
            }
        }
    }

    // Try tput cols and tput lines with redirected TTY or stderr/proc streams
    for path in &["/dev/tty", "/dev/stderr", "/proc/self/fd/2"] {
        if let Ok(file) = std::fs::File::open(path) {
            if let Ok(file_clone) = file.try_clone() {
                if let Ok(output_cols) = std::process::Command::new("tput")
                    .arg("cols")
                    .stdin(file)
                    .output()
                {
                    if let Ok(output_lines) = std::process::Command::new("tput")
                        .arg("lines")
                        .stdin(file_clone)
                        .output()
                    {
                        let cols_str = String::from_utf8_lossy(&output_cols.stdout).trim().to_string();
                        let lines_str = String::from_utf8_lossy(&output_lines.stdout).trim().to_string();
                        if let (Ok(cols), Ok(rows)) = (cols_str.parse::<u16>(), lines_str.parse::<u16>()) {
                            if cols > 0 && rows > 0 {
                                return (cols, rows);
                            }
                        }
                    }
                }
            }
        }
    }

    crossterm::terminal::size().unwrap_or((80, 24))
}

pub fn run_tui_on_main_thread(
    state: Arc<Mutex<TuiState>>,
    comp_handle: std::thread::JoinHandle<Result<(), crate::compressor::ZipError>>,
) -> Result<(), crate::compressor::ZipError> {
    let mut guard = TerminalGuard::new().map_err(crate::compressor::ZipError::Io)?;

    // Get initial terminal size
    let (w, h) = query_initial_size();
    {
        let mut guard_state = state.lock().unwrap();
        guard_state.terminal_cols = w;
        guard_state.terminal_rows = h;
    }
    
    let tick_rate = std::time::Duration::from_millis(100);
    loop {
        // Listen for terminal events with a poll tick rate
        if event::poll(tick_rate).unwrap_or(false) {
            // Drain all pending events to keep event handling responsive and avoid queue lag
            while event::poll(std::time::Duration::from_millis(0)).unwrap_or(false) {
                match event::read().unwrap() {
                    Event::Key(key) => {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                                // Cleanup output file and exit utility cleanly
                                crate::cleanup_output_file();
                                drop(guard);
                                std::process::exit(2);
                            }
                            KeyCode::Char('p') | KeyCode::Char('P') => {
                                let current = crate::IS_PAUSED.load(Ordering::Relaxed);
                                crate::IS_PAUSED.store(!current, Ordering::Relaxed);
                            }
                            KeyCode::Char('-') => {
                                let current = crate::THROTTLE_DELAY_MS.load(Ordering::Relaxed);
                                let new_val = std::cmp::min(current + 50, 500);
                                crate::THROTTLE_DELAY_MS.store(new_val, Ordering::Relaxed);
                            }
                            KeyCode::Char('+') | KeyCode::Char('=') => {
                                let current = crate::THROTTLE_DELAY_MS.load(Ordering::Relaxed);
                                let new_val = current.saturating_sub(50);
                                crate::THROTTLE_DELAY_MS.store(new_val, Ordering::Relaxed);
                            }
                            KeyCode::Up => {
                                let offset = crate::LOG_SCROLL_OFFSET.load(Ordering::Relaxed);
                                crate::LOG_SCROLL_OFFSET.store(offset.saturating_sub(1), Ordering::Relaxed);
                            }
                            KeyCode::Down => {
                                let offset = crate::LOG_SCROLL_OFFSET.load(Ordering::Relaxed);
                                crate::LOG_SCROLL_OFFSET.store(offset + 1, Ordering::Relaxed);
                            }
                            _ => {}
                        }
                    }
                    Event::Resize(w, h) => {
                        let mut state_guard = state.lock().unwrap();
                        state_guard.terminal_cols = w;
                        state_guard.terminal_rows = h;
                    }
                    _ => {}
                }
            }
        }

        let is_complete = {
            let mut state_guard = state.lock().unwrap();

            // Append current speed to history
            let now = Instant::now();
            let duration = now.duration_since(state_guard.last_speed_update).as_secs_f64();
            let total_in = match state_guard.mode {
                TuiMode::Split => state_guard.stripes.iter().map(|s| s.bytes_processed).sum::<usize>(),
                TuiMode::Stream => state_guard.bytes_read,
            };
            let speed = if duration > 0.0 {
                let delta_in = total_in.saturating_sub(state_guard.prev_total_in);
                delta_in as f64 / duration
            } else {
                0.0
            };
            state_guard.prev_total_in = total_in;
            state_guard.last_speed_update = now;

            state_guard.speed_history.push(speed);
            if state_guard.speed_history.len() > 35 {
                state_guard.speed_history.remove(0);
            }

            let _ = draw_tui(&mut guard.terminal, &state_guard);
            state_guard.is_complete
        };

        if is_complete {
            break;
        }

        if comp_handle.is_finished() {
            break;
        }
    }

    match comp_handle.join() {
        Ok(res) => res,
        Err(_) => Err(crate::compressor::ZipError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Compression thread panicked",
        ))),
    }
}

fn render_history_chart(history: &[f64], height: usize) -> Vec<String> {
    let mut chart_lines = vec![String::new(); height];
    let max_val = history.iter().copied().fold(0.0, f64::max);
    let scale = if max_val > 0.0 { max_val } else { 1.0 };

    let (top_label, bot_label) = if max_val >= 1024.0 * 1024.0 * 1024.0 {
        (
            format!("{:4.1}G ┼", max_val / (1024.0 * 1024.0 * 1024.0)),
            "0.0G ┼".to_string(),
        )
    } else if max_val >= 1024.0 * 1024.0 {
        (
            format!("{:4.1}M ┼", max_val / (1024.0 * 1024.0)),
            "0.0M ┼".to_string(),
        )
    } else if max_val >= 1024.0 {
        (
            format!("{:4.1}K ┼", max_val / 1024.0),
            "0.0K ┼".to_string(),
        )
    } else {
        (
            format!("{:4.0}B ┼", max_val),
            "  0B ┼".to_string(),
        )
    };
    let mid_label = "     │".to_string();

    for r in 0..height {
        let label = if r == 0 {
            &top_label
        } else if r == height - 1 {
            &bot_label
        } else {
            &mid_label
        };

        let mut row_str = String::new();
        row_str.push_str(label);

        let data_width = 27usize;
        let padding = data_width.saturating_sub(history.len());
        row_str.push_str(&" ".repeat(padding));

        let start_idx = history.len().saturating_sub(data_width);
        if !history.is_empty() {
            for &val in &history[start_idx..] {
                let pct = val / scale;
                let val_height = pct * height as f64;
                let threshold = (height - 1 - r) as f64;
                
                let diff = val_height - threshold;
                let ch = if diff >= 1.0 {
                    '█'
                } else if diff >= 0.875 {
                    '█'
                } else if diff >= 0.75 {
                    '▇'
                } else if diff >= 0.625 {
                    '▆'
                } else if diff >= 0.5 {
                    '▅'
                } else if diff >= 0.375 {
                    '▄'
                } else if diff >= 0.25 {
                    '▃'
                } else if diff >= 0.125 {
                    '▂'
                } else {
                    ' '
                };
                row_str.push(ch);
            }
        } else {
            row_str.push_str(&" ".repeat(data_width));
        }

        chart_lines[r] = row_str;
    }
    chart_lines
}

pub fn draw_tui<B: ratatui::backend::Backend>(
    terminal: &mut ratatui::Terminal<B>,
    state: &TuiState,
) -> Result<(), std::io::Error> {
    // Standardized/mockable elapsed time for layout consistency in tests
    #[cfg(test)]
    let elapsed = 1.234;
    #[cfg(not(test))]
    let elapsed = state.start_time.elapsed().as_secs_f64();

    // Style Colors: LCARS Palette
    let style_orange = Style::default().fg(Color::Indexed(208));
    let style_purple = Style::default().fg(Color::Indexed(147));
    let style_cyan = Style::default().fg(Color::Indexed(117));
    let style_yellow = Style::default().fg(Color::Indexed(220));

    let pause_status = if crate::IS_PAUSED.load(Ordering::Relaxed) {
        "PAUSED "
    } else {
        "RUNNING"
    };
    let throttle_delay = crate::THROTTLE_DELAY_MS.load(Ordering::Relaxed);

    terminal.draw(|f| {
        let area = f.size();
        let cols = area.width;
        let rows = area.height;

        // Clear the screen to remove previous terminal remnants
        f.render_widget(Clear, area);

        if cols < 80 || rows < 22 {
            let warning_lines = vec![
                Line::from(Span::styled("Terminal size too small.", Style::default().fg(Color::Red))),
                Line::from(Span::styled(format!("Current: {}x{}", cols, rows), Style::default().fg(Color::Red))),
                Line::from(Span::styled("Please resize to at least 80x22.", Style::default().fg(Color::Yellow))),
            ];
            let paragraph = Paragraph::new(warning_lines);
            f.render_widget(paragraph, area);
            return;
        }

        let pad_left = (cols as usize).saturating_sub(80) / 2;
        let pad_top = (rows as usize).saturating_sub(22) / 2;

        let rect = Rect::new(
            pad_left as u16,
            pad_top as u16,
            80,
            22,
        );

        // Split into 22 individual rows
        let row_rects = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Length(1); 22])
            .split(rect);

        // Row helper for 3-column layout (left border, left content, mid divider, right content, right border)
        let render_row = |f: &mut ratatui::Frame, y_rect: Rect, left_line: Line, right_line: Line| {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(2),  // "│ "
                    Constraint::Length(38), // Left content
                    Constraint::Length(3),  // " │ "
                    Constraint::Length(35), // Right content
                    Constraint::Length(2),  // " │"
                ])
                .split(y_rect);
            
            f.render_widget(Paragraph::new(Line::from(Span::styled("│ ", style_orange))), cols[0]);
            f.render_widget(Paragraph::new(left_line), cols[1]);
            f.render_widget(Paragraph::new(Line::from(Span::styled(" │ ", style_orange))), cols[2]);
            f.render_widget(Paragraph::new(right_line), cols[3]);
            f.render_widget(Paragraph::new(Line::from(Span::styled(" │", style_orange))), cols[4]);
        };

        // Row 0: Top border
        let top_border = Paragraph::new(Line::from(Span::styled(
            "┌─── LCARS COMMAND PANEL ─ [ SYSTEM: ACTIVE ] ─────────────────────────────────┐",
            style_orange
        )));
        f.render_widget(top_border, row_rects[0]);

        match state.mode {
            TuiMode::Split => {
                let mut total_in = 0;
                let mut total_out = 0;

                for stripe in &state.stripes {
                    total_in += stripe.bytes_processed;
                    total_out += stripe.bytes_written;
                }

                let speed = if elapsed > 0.0 {
                    total_in as f64 / elapsed
                } else {
                    0.0
                };

                let eta_str = if total_in == 0 {
                    "Estimating...".to_string()
                } else if total_in >= state.total_input_size {
                    "0s (Complete)".to_string()
                } else if speed > 0.0 {
                    let remaining = state.total_input_size.saturating_sub(total_in);
                    let eta_secs = (remaining as f64 / speed).round() as usize;
                    format!("{}s", eta_secs)
                } else {
                    "--".to_string()
                };

                let ratio = if total_out > 0 {
                    total_in as f64 / total_out as f64
                } else {
                    1.0
                };

                // Row 1: Ingested & Speed (Styled Labels)
                let left_line_1 = Line::from(vec![
                    Span::styled("Ingested : ", style_purple),
                    Span::styled(format!("{:7.2}", total_in as f64 / (1024.0 * 1024.0)), style_cyan),
                    Span::styled(" MB / ", style_purple),
                    Span::styled(format!("{:7.2}", state.total_input_size as f64 / (1024.0 * 1024.0)), style_cyan),
                    Span::styled(" MB", style_purple),
                ]);
                let right_line_1 = Line::from(vec![
                    Span::styled("Speed: ", style_purple),
                    Span::styled(format!("{:5.1}", speed / (1024.0 * 1024.0)), style_cyan),
                    Span::styled(" MB/s", style_purple),
                ]);
                render_row(f, row_rects[1], left_line_1, right_line_1);

                // Row 2: Output & Time (Styled Labels)
                let left_line_2 = Line::from(vec![
                    Span::styled("Output   : ", style_purple),
                    Span::styled(format!("{:7.2}", total_out as f64 / (1024.0 * 1024.0)), style_cyan),
                    Span::styled(" MB (", style_purple),
                    Span::styled(format!("{:5.2}x", ratio), style_yellow),
                    Span::styled(" Ratio)", style_purple),
                ]);
                let right_line_2 = Line::from(vec![
                    Span::styled("Time : ", style_purple),
                    Span::styled(format!("{:5.1}s", elapsed), style_cyan),
                    Span::styled(" (ETA: ", style_purple),
                    Span::styled(format!("{:<13}", eta_str), style_yellow),
                    Span::styled(")", style_purple),
                ]);
                render_row(f, row_rects[2], left_line_2, right_line_2);

                // Row 3: Divider 1
                f.render_widget(Paragraph::new(Line::from(Span::styled(
                    "├────────────────────────────────────────┬─────────────────────────────────────┤",
                    style_orange
                ))), row_rects[3]);

                // Row 4: Body Headers
                render_row(
                    f,
                    row_rects[4],
                    Line::from(Span::styled("STRIPE SECTORS PROGRESS", style_purple)),
                    Line::from(Span::styled("INGEST SPEED HISTORY (35s)", style_purple))
                );

                // Rows 5..10: Sectors Progress & History Chart
                let chart_lines = render_history_chart(&state.speed_history, 6);
                for r in 0..6 {
                    let left_line = if r < state.stripes.len() {
                        let stripe = &state.stripes[r];
                        let pct = if stripe.total_bytes > 0 {
                            (stripe.bytes_processed as f64 / stripe.total_bytes as f64) * 100.0
                        } else {
                            0.0
                        };
                        let bar_len = 15;
                        let filled = std::cmp::min(((pct / 100.0) * bar_len as f64) as usize, bar_len);
                        let bar: String = std::iter::repeat('█')
                            .take(filled)
                            .chain(std::iter::repeat('░').take(bar_len - filled))
                            .collect();
                        let stripe_ratio = if stripe.bytes_written > 0 {
                            stripe.bytes_processed as f64 / stripe.bytes_written as f64
                        } else {
                            1.0
                        };
                        Line::from(vec![
                            Span::styled(format!("Sec {:02}: ", stripe.id), style_purple),
                            Span::styled(format!("[{}] ", bar), style_cyan),
                            Span::styled(format!("{:3.0}%", pct), style_yellow),
                            Span::styled(format!(" ({:4.1}x)", stripe_ratio), style_cyan),
                        ])
                    } else {
                        Line::from("")
                    };

                    let right_line = Line::from(Span::styled(chart_lines[r].clone(), style_yellow));
                    render_row(f, row_rects[5 + r], left_line, right_line);
                }

                // Row 11: Divider 2
                f.render_widget(Paragraph::new(Line::from(Span::styled(
                    "├────────────────────────────────────────┼─────────────────────────────────────┤",
                    style_orange
                ))), row_rects[11]);
            }
            TuiMode::Stream => {
                let speed_in = if elapsed > 0.0 {
                    state.bytes_read as f64 / elapsed
                } else {
                    0.0
                };
                let speed_out = if elapsed > 0.0 {
                    state.bytes_written as f64 / elapsed
                } else {
                    0.0
                };

                let ratio = if state.bytes_written > 0 {
                    state.bytes_read as f64 / state.bytes_written as f64
                } else {
                    1.0
                };

                let proj_1m = speed_in * 60.0;
                let proj_5m = speed_in * 300.0;

                // Row 1: Ingested & Speed
                let left_line_1 = Line::from(vec![
                    Span::styled("Ingested : ", style_purple),
                    Span::styled(format!("{:7.2}", state.bytes_read as f64 / (1024.0 * 1024.0)), style_cyan),
                    Span::styled(" MB", style_purple),
                ]);
                let right_line_1 = Line::from(vec![
                    Span::styled("Speed: ", style_purple),
                    Span::styled(format!("{:5.1}", speed_in / (1024.0 * 1024.0)), style_cyan),
                    Span::styled(" MB/s", style_purple),
                ]);
                render_row(f, row_rects[1], left_line_1, right_line_1);

                // Row 2: Output & Speed
                let left_line_2 = Line::from(vec![
                    Span::styled("Output   : ", style_purple),
                    Span::styled(format!("{:7.2}", state.bytes_written as f64 / (1024.0 * 1024.0)), style_cyan),
                    Span::styled(" MB (", style_purple),
                    Span::styled(format!("{:5.2}x", ratio), style_yellow),
                    Span::styled(" Ratio)", style_purple),
                ]);
                let right_line_2 = Line::from(vec![
                    Span::styled("Speed: ", style_purple),
                    Span::styled(format!("{:5.1}", speed_out / (1024.0 * 1024.0)), style_cyan),
                    Span::styled(" MB/s", style_purple),
                ]);
                render_row(f, row_rects[2], left_line_2, right_line_2);

                // Row 3: Divider 1
                f.render_widget(Paragraph::new(Line::from(Span::styled(
                    "├────────────────────────────────────────┬─────────────────────────────────────┤",
                    style_orange
                ))), row_rects[3]);

                // Row 4: Body Headers
                render_row(
                    f,
                    row_rects[4],
                    Line::from(Span::styled("TRANSPORTER BUFFER CAPACITY", style_purple)),
                    Line::from(Span::styled("INGEST SPEED HISTORY (35s)", style_purple))
                );

                // Rows 5..10: Transporter Buffer & Projections & Speed History Chart
                let chart_lines = render_history_chart(&state.speed_history, 6);
                for r in 0..6 {
                    let left_line = if r == 0 {
                        let queue_list = if state.input_queue.is_empty() {
                            "[]".to_string()
                        } else {
                            let mut s = format!("{:?}", state.input_queue);
                            if s.len() > 22 {
                                let mut items = Vec::new();
                                for x in &state.input_queue {
                                    let next_s = format!("{:?}", items);
                                    if next_s.len() > 14 {
                                        items.push("..".to_string());
                                        break;
                                    }
                                    items.push(x.to_string());
                                }
                                s = format!("[{}]", items.join(","));
                            }
                            s
                        };
                        Line::from(vec![
                            Span::styled("Queue: ", style_purple),
                            Span::styled(queue_list, style_cyan),
                            Span::styled(format!(" ({}/{} blk)", state.queue_depth, state.queue_capacity), style_yellow),
                        ])
                    } else if r == 1 {
                        let mut w_parts = Vec::new();
                        if state.workers.len() > 0 {
                            let w = &state.workers[0];
                            w_parts.push(Span::styled(format!("W00:[{:<4}] ", w.status), style_purple));
                            w_parts.push(Span::styled(w.current_chunk.map(|c| format!("#{:<3}", c)).unwrap_or_else(|| "-- ".to_string()), style_cyan));
                        }
                        if state.workers.len() > 1 {
                            w_parts.push(Span::styled("   ", style_purple));
                            let w = &state.workers[1];
                            w_parts.push(Span::styled(format!("W01:[{:<4}] ", w.status), style_purple));
                            w_parts.push(Span::styled(w.current_chunk.map(|c| format!("#{:<3}", c)).unwrap_or_else(|| "-- ".to_string()), style_cyan));
                        }
                        Line::from(w_parts)
                    } else if r == 2 {
                        let mut w_parts = Vec::new();
                        if state.workers.len() > 2 {
                            let w = &state.workers[2];
                            w_parts.push(Span::styled(format!("W02:[{:<4}] ", w.status), style_purple));
                            w_parts.push(Span::styled(w.current_chunk.map(|c| format!("#{:<3}", c)).unwrap_or_else(|| "-- ".to_string()), style_cyan));
                        }
                        if state.workers.len() > 3 {
                            w_parts.push(Span::styled("   ", style_purple));
                            let w = &state.workers[3];
                            w_parts.push(Span::styled(format!("W03:[{:<4}] ", w.status), style_purple));
                            w_parts.push(Span::styled(w.current_chunk.map(|c| format!("#{:<3}", c)).unwrap_or_else(|| "-- ".to_string()), style_cyan));
                        }
                        Line::from(w_parts)
                    } else if r == 3 {
                        let out_queue_list = if state.output_buffer.is_empty() {
                            "[]".to_string()
                        } else {
                            let mut s = format!("{:?}", state.output_buffer);
                            if s.len() > 18 {
                                let mut items = Vec::new();
                                for x in &state.output_buffer {
                                    let next_s = format!("{:?}", items);
                                    if next_s.len() > 10 {
                                        items.push("..".to_string());
                                        break;
                                    }
                                    items.push(x.to_string());
                                }
                                s = format!("[{}]", items.join(","));
                            }
                            s
                        };
                        Line::from(vec![
                            Span::styled("Out Q: ", style_purple),
                            Span::styled(out_queue_list, style_cyan),
                            Span::styled(format!(" (Next: #{})", state.next_expected_seq), style_yellow),
                        ])
                    } else if r == 4 {
                        let gap_str = if !state.output_buffer.is_empty() && state.output_buffer[0] > state.next_expected_seq {
                            format!("GAP AT #{}", state.next_expected_seq)
                        } else {
                            "NO GAPS".to_string()
                        };
                        let style = if gap_str.starts_with("GAP") {
                            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::Green)
                        };
                        Line::from(vec![
                            Span::styled("Gaps : ", style_purple),
                            Span::styled(gap_str, style),
                        ])
                    } else {
                        Line::from(vec![
                            Span::styled("Target 1m: ", style_purple),
                            Span::styled(format!("{:5.1}M", proj_1m / (1024.0 * 1024.0)), style_cyan),
                            Span::styled("  5m: ", style_purple),
                            Span::styled(format!("{:5.1}M", proj_5m / (1024.0 * 1024.0)), style_cyan),
                        ])
                    };

                    let right_line = Line::from(Span::styled(chart_lines[r].clone(), style_yellow));
                    render_row(f, row_rects[5 + r], left_line, right_line);
                }

                // Row 11: Divider 2
                f.render_widget(Paragraph::new(Line::from(Span::styled(
                    "├────────────────────────────────────────┼─────────────────────────────────────┤",
                    style_orange
                ))), row_rects[11]);
            }
        }

        // Row 12: Logs Header Panel (with Knobs info on the right)
        let log_header_left = Line::from(vec![
            Span::styled("SYSTEM LOG MESSAGES (SCROLL: ", style_purple),
            Span::styled("▲", Style::default().fg(Color::Black).bg(Color::Indexed(147)).add_modifier(Modifier::BOLD)),
            Span::styled("/", style_purple),
            Span::styled("▼", Style::default().fg(Color::Black).bg(Color::Indexed(147)).add_modifier(Modifier::BOLD)),
            Span::styled(")", style_purple),
        ]);
        let pool_size = if state.mode == TuiMode::Split {
            state.stripes.len()
        } else {
            state.workers.len()
        };
        let chunk_size_str = match state.mode {
            TuiMode::Split => {
                let size = if !state.stripes.is_empty() && state.stripes[0].total_bytes > 0 {
                    state.stripes[0].total_bytes
                } else if !state.stripes.is_empty() {
                    state.total_input_size / state.stripes.len()
                } else {
                    0
                };
                format!("{:.1}MB", size as f64 / (1024.0 * 1024.0))
            }
            TuiMode::Stream => "4.0MB".to_string(),
        };
        let qcap = state.queue_capacity;
        let knobs_right = Line::from(vec![
            Span::styled("Knobs: ", style_purple),
            Span::styled(format!("P:{} ", pool_size), style_yellow),
            Span::styled(format!("C:{} ", chunk_size_str.replace("MB", "M")), style_yellow),
            Span::styled(format!("Q:{} ", qcap), style_yellow),
            Span::styled(format!("L:{}", state.level), style_yellow),
        ]);
        render_row(f, row_rects[12], log_header_left, knobs_right);

        // Rows 13..17: 5 Scrollable Log Content Rows (full-width spanning)
        let logs = if let Ok(buffer) = crate::get_log_buffer().lock() {
            buffer.clone()
        } else {
            Vec::new()
        };

        let scroll_offset = crate::LOG_SCROLL_OFFSET.load(Ordering::Relaxed);
        let max_offset = logs.len().saturating_sub(5);
        let offset = std::cmp::min(scroll_offset, max_offset);
        crate::LOG_SCROLL_OFFSET.store(offset, Ordering::Relaxed); // Cap/sync offset state

        for r in 0..5 {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(2),  // "│ "
                    Constraint::Length(76), // Full-width log content area
                    Constraint::Length(2),  // " │"
                ])
                .split(row_rects[13 + r]);

            f.render_widget(Paragraph::new(Line::from(Span::styled("│ ", style_orange))), cols[0]);
            
            if offset + r < logs.len() {
                let line_str = &logs[offset + r];
                let truncated_line = if line_str.len() > 76 {
                    format!("{}...", &line_str[..73])
                } else {
                    line_str.clone()
                };
                f.render_widget(Paragraph::new(Line::from(Span::styled(truncated_line, style_cyan))), cols[1]);
            } else {
                f.render_widget(Paragraph::new(Line::from("")), cols[1]);
            }

            f.render_widget(Paragraph::new(Line::from(Span::styled(" │", style_orange))), cols[2]);
        }

        // Row 18: Divider 3 (Log panel border)
        f.render_widget(Paragraph::new(Line::from(Span::styled(
            "├──────────────────────────────────────────────────────────────────────────────┤",
            style_orange
        ))), row_rects[18]);

        // Row 19: Controls 1 (Styled Buttons & States)
        let left_line_1 = Line::from(vec![
            Span::styled("CONTROLS: ", style_purple),
            Span::styled("[", style_purple),
            Span::styled("P", Style::default().fg(Color::Black).bg(Color::Indexed(147)).add_modifier(Modifier::BOLD)),
            Span::styled("] Pause  ", style_purple),
            Span::styled("[", style_purple),
            Span::styled("-", Style::default().fg(Color::Black).bg(Color::Indexed(147)).add_modifier(Modifier::BOLD)),
            Span::styled("] Slow Down", style_purple),
        ]);
        let filled_slider = ((throttle_delay as f64 / 500.0) * 10.0).round() as usize;
        let slider_bar: String = std::iter::repeat('█').take(filled_slider)
            .chain(std::iter::repeat('░').take(10 - filled_slider))
            .collect();
        let right_line_1 = Line::from(vec![
            Span::styled("Throt: ", style_purple),
            Span::styled(format!("[{}] ", slider_bar), style_cyan),
            Span::styled(format!("{:3}ms", throttle_delay), style_yellow),
        ]);
        render_row(f, row_rects[19], left_line_1, right_line_1);

        // Row 20: Controls 2
        let left_line_2 = Line::from(vec![
            Span::styled("          ", style_purple),
            Span::styled("[", style_purple),
            Span::styled("+", Style::default().fg(Color::Black).bg(Color::Indexed(147)).add_modifier(Modifier::BOLD)),
            Span::styled("] Speed Up  ", style_purple),
            Span::styled("[", style_purple),
            Span::styled("Q", Style::default().fg(Color::White).bg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled("] Abort", style_purple),
        ]);
        let right_line_2 = Line::from(vec![
            Span::styled("        STATE   : ", style_purple),
            Span::styled(format!("{:7}", pause_status), if pause_status.trim() == "PAUSED" { Style::default().fg(Color::Red).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::Green).add_modifier(Modifier::BOLD) }),
        ]);
        render_row(f, row_rects[20], left_line_2, right_line_2);

        // Row 21: Bottom border
        f.render_widget(Paragraph::new(Line::from(Span::styled(
            "╰────────────────────────────────────────┴─────────────────────────────────────╯",
            style_orange
        ))), row_rects[21]);
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn get_buffer_string(backend: &TestBackend) -> String {
        let buffer = backend.buffer();
        let mut output = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                let cell = buffer.get(x, y);
                output.push_str(cell.symbol());
            }
            if y < buffer.area.height - 1 {
                output.push('\n');
            }
        }
        output
    }

    #[test]
    fn test_tui_layout_split_mode_snapshot() {
        let mut state = TuiState::new_split(4, 400 * 1024, 6);
        
        state.stripes[0].total_bytes = 102400;
        state.stripes[0].bytes_processed = 102400;
        state.stripes[0].bytes_written = 40960;

        state.stripes[1].total_bytes = 102400;
        state.stripes[1].bytes_processed = 51200;
        state.stripes[1].bytes_written = 20480;

        state.stripes[2].total_bytes = 102400;
        state.stripes[2].bytes_processed = 0;
        state.stripes[2].bytes_written = 0;

        state.stripes[3].total_bytes = 102400;
        state.stripes[3].bytes_processed = 25600;
        state.stripes[3].bytes_written = 10240;

        state.speed_history = vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];

        let backend = TestBackend::new(80, 22);
        let mut terminal = Terminal::new(backend).unwrap();
        draw_tui(&mut terminal, &state).unwrap();
        let clean_output = get_buffer_string(terminal.backend());

        insta::assert_snapshot!(clean_output);
    }

    #[test]
    fn test_tui_layout_stream_mode_snapshot() {
        let mut state = TuiState::new_stream(8, 0, 4, 6);
        state.bytes_read = 50 * 1024 * 1024;
        state.bytes_written = 20 * 1024 * 1024;
        state.queue_depth = 3;

        state.speed_history = vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];

        let backend = TestBackend::new(80, 22);
        let mut terminal = Terminal::new(backend).unwrap();
        draw_tui(&mut terminal, &state).unwrap();
        let clean_output = get_buffer_string(terminal.backend());

        insta::assert_snapshot!(clean_output);
    }

    #[test]
    fn test_tui_layout_split_overflow() {
        let mut state = TuiState::new_split(1, 100 * 1024, 6);
        state.stripes[0].total_bytes = 100000;
        state.stripes[0].bytes_processed = 120000; // 120%
        state.stripes[0].bytes_written = 40000;

        let backend = TestBackend::new(80, 22);
        let mut terminal = Terminal::new(backend).unwrap();
        draw_tui(&mut terminal, &state).unwrap();
        let clean_output = get_buffer_string(terminal.backend());
        assert!(clean_output.contains("120%"));
    }

    #[test]
    fn test_tui_layout_stream_overflow() {
        let mut state = TuiState::new_stream(8, 0, 4, 6);
        state.queue_depth = 12; // Exceeds cap (8)

        let backend = TestBackend::new(80, 22);
        let mut terminal = Terminal::new(backend).unwrap();
        draw_tui(&mut terminal, &state).unwrap();
        let clean_output = get_buffer_string(terminal.backend());
        assert!(clean_output.contains("12/8 blk"));
    }

    #[test]
    fn test_query_initial_size_matches_stty() {
        let (cols, rows) = query_initial_size();
        
        let file = std::fs::File::open("/dev/tty").expect("Unable to open /dev/tty");
        let output = std::process::Command::new("stty")
            .arg("size")
            .stdin(file)
            .output()
            .expect("Failed to run stty size");
        let out_str = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = out_str.split_whitespace().collect();
        if parts.len() == 2 {
            if let (Ok(expected_rows), Ok(expected_cols)) = (parts[0].parse::<u16>(), parts[1].parse::<u16>()) {
                assert_eq!(cols, expected_cols, "cols must match stty cols");
                assert_eq!(rows, expected_rows, "rows must match stty rows");
            }
        }
    }

    #[test]
    fn test_tui_centering_coordinates() {
        let state = TuiState::new_split(1, 100 * 1024, 6);
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        draw_tui(&mut terminal, &state).unwrap();
        
        let buffer = terminal.backend().buffer();
        let mut row_2_str = String::new();
        for x in 0..80 {
            row_2_str.push_str(buffer.get(x, 1).symbol());
        }
        assert!(
            row_2_str.contains("┌───"),
            "Row 2 (0-indexed 1) must contain the top border ┌───, got: {:?}",
            row_2_str
        );
    }

    #[test]
    fn test_tui_centering_coordinates_non_standard() {
        let mut state = TuiState::new_split(1, 100 * 1024, 6);
        state.terminal_cols = 120;
        state.terminal_rows = 40;

        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        draw_tui(&mut terminal, &state).unwrap();

        let buffer = terminal.backend().buffer();
        let mut row_10_str = String::new();
        for x in 0..120 {
            row_10_str.push_str(buffer.get(x, 9).symbol());
        }
        assert!(
            row_10_str[20..].starts_with("┌───"),
            "Row 10 (0-indexed 9) starting at col 20 must contain the top border ┌───, got: {:?}",
            row_10_str
        );
    }

    #[test]
    fn test_tui_layout_perfect_alignment() {
        // Test alignment in Split Mode
        {
            let mut state = TuiState::new_split(4, 400 * 1024, 6);
            state.speed_history = vec![1.0, 2.0, 3.0];
            let backend = TestBackend::new(80, 22);
            let mut terminal = Terminal::new(backend).unwrap();
            draw_tui(&mut terminal, &state).unwrap();
            let clean_output = get_buffer_string(terminal.backend());
            
            for line in clean_output.lines() {
                if line.starts_with('┌') || line.starts_with('│') || line.starts_with('├') || line.starts_with('╰') {
                    assert_eq!(
                        line.chars().count(),
                        80,
                        "Split Mode line is not exactly 80 characters wide: {:?} (length {})",
                        line,
                        line.chars().count()
                    );
                }
            }
        }

        // Test alignment in Stream Mode
        {
            let mut state = TuiState::new_stream(8, 0, 4, 6);
            state.speed_history = vec![1.0, 2.0, 3.0];
            let backend = TestBackend::new(80, 22);
            let mut terminal = Terminal::new(backend).unwrap();
            draw_tui(&mut terminal, &state).unwrap();
            let clean_output = get_buffer_string(terminal.backend());
            
            for line in clean_output.lines() {
                if line.starts_with('┌') || line.starts_with('│') || line.starts_with('├') || line.starts_with('╰') {
                    assert_eq!(
                        line.chars().count(),
                        80,
                        "Stream Mode line is not exactly 80 characters wide: {:?} (length {})",
                        line,
                        line.chars().count()
                    );
                }
            }
        }
    }
}
