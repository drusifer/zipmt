use std::io::Write;
use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering;
use std::time::Instant;

use crossterm::event::{self, Event, KeyCode};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use ratatui::style::{Color, Style};
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
}

impl TuiState {
    pub fn new_split(total_stripes: usize, total_input_size: usize) -> Self {
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
        }
    }

    pub fn new_stream(queue_capacity: usize, total_input_size: usize) -> Self {
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
            let elapsed = state_guard.start_time.elapsed().as_secs_f64();
            let total_in = match state_guard.mode {
                TuiMode::Split => state_guard.stripes.iter().map(|s| s.bytes_processed).sum::<usize>(),
                TuiMode::Stream => state_guard.bytes_read,
            };
            let speed = if elapsed > 0.0 {
                total_in as f64 / elapsed
            } else {
                0.0
            };

            if !crate::IS_PAUSED.load(Ordering::Relaxed) {
                state_guard.speed_history.push(speed);
                if state_guard.speed_history.len() > 35 {
                    state_guard.speed_history.remove(0);
                }
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
    if history.is_empty() {
        for line in chart_lines.iter_mut() {
            *line = " ".repeat(35);
        }
        return chart_lines;
    }

    let max_val = history.iter().copied().fold(0.0, f64::max);
    let scale = if max_val > 0.0 { max_val } else { 1.0 };

    for r in 0..height {
        let mut row_str = String::new();
        let padding = 35usize.saturating_sub(history.len());
        row_str.push_str(&" ".repeat(padding));

        for &val in history {
            let pct = val / scale;
            let val_height = pct * height as f64;
            let threshold = r as f64;
            
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
        chart_lines[height - 1 - r] = row_str;
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

        if cols < 80 || rows < 15 {
            let warning_lines = vec![
                Line::from(Span::styled("Terminal size too small.", Style::default().fg(Color::Red))),
                Line::from(Span::styled(format!("Current: {}x{}", cols, rows), Style::default().fg(Color::Red))),
                Line::from(Span::styled("Please resize to at least 80x15.", Style::default().fg(Color::Yellow))),
            ];
            let paragraph = Paragraph::new(warning_lines);
            f.render_widget(paragraph, area);
            return;
        }

        let pad_left = (cols as usize).saturating_sub(80) / 2;
        let pad_top = (rows as usize).saturating_sub(15) / 2;

        let rect = Rect::new(
            pad_left as u16,
            pad_top as u16,
            80,
            15,
        );

        // Split into 15 individual rows
        let row_rects = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Length(1); 15])
            .split(rect);

        // Row helper for 3-column layout (left border, left content, mid divider, right content, right border)
        let render_row = |f: &mut ratatui::Frame, y_rect: Rect, left_span: Span, right_span: Span| {
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
            f.render_widget(Paragraph::new(Line::from(left_span)), cols[1]);
            f.render_widget(Paragraph::new(Line::from(Span::styled(" │ ", style_orange))), cols[2]);
            f.render_widget(Paragraph::new(Line::from(right_span)), cols[3]);
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

                // Row 1: Ingested & Speed
                let left_str_1 = format!(
                    "Ingested : {:7.2} MB / {:7.2} MB",
                    total_in as f64 / (1024.0 * 1024.0),
                    state.total_input_size as f64 / (1024.0 * 1024.0)
                );
                let right_str_1 = format!(
                    "Speed: {:5.1} MB/s",
                    speed / (1024.0 * 1024.0)
                );
                render_row(f, row_rects[1], Span::styled(left_str_1, style_cyan), Span::styled(right_str_1, style_cyan));

                // Row 2: Output & Time
                let left_str_2 = format!(
                    "Output   : {:7.2} MB ({:5.2}x Ratio)",
                    total_out as f64 / (1024.0 * 1024.0),
                    ratio
                );
                let right_str_2 = format!(
                    "Time : {:5.1}s (ETA: {:<13})",
                    elapsed,
                    eta_str
                );
                render_row(f, row_rects[2], Span::styled(left_str_2, style_cyan), Span::styled(right_str_2, style_cyan));

                // Row 3: Divider 1
                f.render_widget(Paragraph::new(Line::from(Span::styled(
                    "├────────────────────────────────────────┬─────────────────────────────────────┤",
                    style_orange
                ))), row_rects[3]);

                // Row 4: Body Headers
                render_row(
                    f,
                    row_rects[4],
                    Span::styled("STRIPE SECTORS PROGRESS", style_purple),
                    Span::styled("INGEST SPEED HISTORY (35s)", style_purple)
                );

                // Rows 5..10: Sectors Progress & History Chart
                let chart_lines = render_history_chart(&state.speed_history, 6);
                for r in 0..6 {
                    let left_span = if r < state.stripes.len() {
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
                        Span::styled(format!(
                            "Sec {:02}: [{}] {:3.0}% ({:4.1}x)",
                            stripe.id,
                            bar,
                            pct,
                            stripe_ratio
                        ), style_cyan)
                    } else {
                        Span::raw("")
                    };

                    let right_span = Span::styled(chart_lines[r].clone(), style_yellow);
                    render_row(f, row_rects[5 + r], left_span, right_span);
                }

                // Row 11: Divider 2
                f.render_widget(Paragraph::new(Line::from(Span::styled(
                    "├────────────────────────────────────────┼─────────────────────────────────────┤",
                    style_orange
                ))), row_rects[11]);

                // Row 12: Controls 1
                let control_left_1 = "CONTROLS: [P] Pause  [-] Slow Down";
                let control_right_1 = format!("STATUS: THROTTLE: {:3}ms", throttle_delay);
                render_row(
                    f,
                    row_rects[12],
                    Span::styled(control_left_1, style_purple),
                    Span::styled(control_right_1, style_purple)
                );

                // Row 13: Controls 2
                let control_left_2 = "          [+] Speed Up  [Q] Abort";
                let control_right_2 = format!("        STATE   : {:7}", pause_status);
                render_row(
                    f,
                    row_rects[13],
                    Span::styled(control_left_2, style_orange),
                    Span::styled(control_right_2, style_orange)
                );

                // Row 14: Bottom border
                f.render_widget(Paragraph::new(Line::from(Span::styled(
                    "╰────────────────────────────────────────┴─────────────────────────────────────╯",
                    style_orange
                ))), row_rects[14]);
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
                let proj_10m = speed_in * 600.0;

                // Row 1: Ingested & Speed
                let left_str = format!(
                    "Ingested : {:7.2} MB",
                    state.bytes_read as f64 / (1024.0 * 1024.0)
                );
                let right_str = format!(
                    "Speed: {:5.1} MB/s",
                    speed_in / (1024.0 * 1024.0)
                );
                render_row(f, row_rects[1], Span::styled(left_str, style_cyan), Span::styled(right_str, style_cyan));

                // Row 2: Output & Speed
                let left_str_2 = format!(
                    "Output   : {:7.2} MB ({:5.2}x Ratio)",
                    state.bytes_written as f64 / (1024.0 * 1024.0),
                    ratio
                );
                let right_str_2 = format!(
                    "Speed: {:5.1} MB/s",
                    speed_out / (1024.0 * 1024.0)
                );
                render_row(f, row_rects[2], Span::styled(left_str_2, style_cyan), Span::styled(right_str_2, style_cyan));

                // Row 3: Divider 1
                f.render_widget(Paragraph::new(Line::from(Span::styled(
                    "├────────────────────────────────────────┬─────────────────────────────────────┤",
                    style_orange
                ))), row_rects[3]);

                // Row 4: Body Headers
                render_row(
                    f,
                    row_rects[4],
                    Span::styled("TRANSPORTER BUFFER CAPACITY", style_purple),
                    Span::styled("INGEST SPEED HISTORY (35s)", style_purple)
                );

                // Rows 5..10: Transporter Buffer & Projections & Speed History Chart
                let chart_lines = render_history_chart(&state.speed_history, 6);
                for r in 0..6 {
                    let left_span = if r == 0 {
                        let cap = state.queue_capacity;
                        let depth = state.queue_depth;
                        let cap_f = if cap > 0 { cap } else { 1 };
                        let bar_len = 15;
                        let filled = std::cmp::min((depth * bar_len) / cap_f, bar_len);
                        let bar: String = std::iter::repeat('█')
                            .take(filled)
                            .chain(std::iter::repeat('░').take(bar_len - filled))
                            .collect();
                        Span::styled(format!(
                            "Buffer: [{}] {:2}/{:2} blk",
                            bar,
                            depth,
                            cap
                        ), style_cyan)
                    } else if r == 2 {
                        Span::styled(format!("1m Ingest Target : {:8.1} MB", proj_1m / (1024.0 * 1024.0)), style_cyan)
                    } else if r == 3 {
                        Span::styled(format!("5m Ingest Target : {:8.1} MB", proj_5m / (1024.0 * 1024.0)), style_cyan)
                    } else if r == 4 {
                        Span::styled(format!("10m Ingest Target: {:8.1} MB", proj_10m / (1024.0 * 1024.0)), style_cyan)
                    } else {
                        Span::raw("")
                    };

                    let right_span = Span::styled(chart_lines[r].clone(), style_yellow);
                    render_row(f, row_rects[5 + r], left_span, right_span);
                }

                // Row 11: Divider 2
                f.render_widget(Paragraph::new(Line::from(Span::styled(
                    "├────────────────────────────────────────┼─────────────────────────────────────┤",
                    style_orange
                ))), row_rects[11]);

                // Row 12: Controls 1
                let control_left_1 = "CONTROLS: [P] Pause  [-] Slow Down";
                let control_right_1 = format!("STATUS: THROTTLE: {:3}ms", throttle_delay);
                render_row(
                    f,
                    row_rects[12],
                    Span::styled(control_left_1, style_purple),
                    Span::styled(control_right_1, style_purple)
                );

                // Row 13: Controls 2
                let control_left_2 = "          [+] Speed Up  [Q] Abort";
                let control_right_2 = format!("        STATE   : {:7}", pause_status);
                render_row(
                    f,
                    row_rects[13],
                    Span::styled(control_left_2, style_orange),
                    Span::styled(control_right_2, style_orange)
                );

                // Row 14: Bottom border
                f.render_widget(Paragraph::new(Line::from(Span::styled(
                    "╰────────────────────────────────────────┴─────────────────────────────────────╯",
                    style_orange
                ))), row_rects[14]);
            }
        }
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
        let mut state = TuiState::new_split(4, 400 * 1024);
        
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

        let backend = TestBackend::new(80, 15);
        let mut terminal = Terminal::new(backend).unwrap();
        draw_tui(&mut terminal, &state).unwrap();
        let clean_output = get_buffer_string(terminal.backend());

        insta::assert_snapshot!(clean_output);
    }

    #[test]
    fn test_tui_layout_stream_mode_snapshot() {
        let mut state = TuiState::new_stream(8, 0);
        state.bytes_read = 50 * 1024 * 1024;
        state.bytes_written = 20 * 1024 * 1024;
        state.queue_depth = 3;

        state.speed_history = vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];

        let backend = TestBackend::new(80, 15);
        let mut terminal = Terminal::new(backend).unwrap();
        draw_tui(&mut terminal, &state).unwrap();
        let clean_output = get_buffer_string(terminal.backend());

        insta::assert_snapshot!(clean_output);
    }

    #[test]
    fn test_tui_layout_split_overflow() {
        let mut state = TuiState::new_split(1, 100 * 1024);
        state.stripes[0].total_bytes = 100000;
        state.stripes[0].bytes_processed = 120000; // 120%
        state.stripes[0].bytes_written = 40000;

        let backend = TestBackend::new(80, 15);
        let mut terminal = Terminal::new(backend).unwrap();
        draw_tui(&mut terminal, &state).unwrap();
        let clean_output = get_buffer_string(terminal.backend());
        assert!(clean_output.contains("120%"));
    }

    #[test]
    fn test_tui_layout_stream_overflow() {
        let mut state = TuiState::new_stream(8, 0);
        state.queue_depth = 12; // Exceeds cap (8)

        let backend = TestBackend::new(80, 15);
        let mut terminal = Terminal::new(backend).unwrap();
        draw_tui(&mut terminal, &state).unwrap();
        let clean_output = get_buffer_string(terminal.backend());
        assert!(clean_output.contains("12/ 8 blk"));
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
        let state = TuiState::new_split(1, 100 * 1024);
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        draw_tui(&mut terminal, &state).unwrap();
        
        let buffer = terminal.backend().buffer();
        let mut row_5_str = String::new();
        for x in 0..80 {
            row_5_str.push_str(buffer.get(x, 4).symbol());
        }
        assert!(
            row_5_str.contains("┌───"),
            "Row 5 (0-indexed 4) must contain the top border ┌───, got: {:?}",
            row_5_str
        );
    }

    #[test]
    fn test_tui_centering_coordinates_non_standard() {
        let mut state = TuiState::new_split(1, 100 * 1024);
        state.terminal_cols = 120;
        state.terminal_rows = 40;

        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        draw_tui(&mut terminal, &state).unwrap();

        let buffer = terminal.backend().buffer();
        let mut row_13_str = String::new();
        for x in 0..120 {
            row_13_str.push_str(buffer.get(x, 12).symbol());
        }
        assert!(
            row_13_str[20..].starts_with("┌───"),
            "Row 13 (0-indexed 12) starting at col 20 must contain the top border ┌───, got: {:?}",
            row_13_str
        );
    }

    #[test]
    fn test_tui_layout_perfect_alignment() {
        // Test alignment in Split Mode
        {
            let mut state = TuiState::new_split(4, 400 * 1024);
            state.speed_history = vec![1.0, 2.0, 3.0];
            let backend = TestBackend::new(80, 15);
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
            let mut state = TuiState::new_stream(8, 0);
            state.speed_history = vec![1.0, 2.0, 3.0];
            let backend = TestBackend::new(80, 15);
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
