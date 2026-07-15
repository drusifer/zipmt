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
use ratatui::widgets::{Paragraph, Clear, Block, Borders, BorderType};

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
    pub total_chunks_compressed: u64,
    pub total_compress_time_ms: f64,
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
            total_chunks_compressed: 0,
            total_compress_time_ms: 0.0,
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
            total_chunks_compressed: 0,
            total_compress_time_ms: 0.0,
        }
    }

    pub fn update_chunk_time(&mut self, duration: std::time::Duration) {
        self.total_chunks_compressed += 1;
        self.total_compress_time_ms += duration.as_secs_f64() * 1000.0;
    }

    pub fn get_avg_chunk_time_ms(&self) -> f64 {
        if self.total_chunks_compressed > 0 {
            self.total_compress_time_ms / self.total_chunks_compressed as f64
        } else {
            0.0
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
                            KeyCode::Char('[') => {
                                let current = crate::COMPRESSION_LEVEL.load(Ordering::Relaxed);
                                if current > 1 {
                                    crate::COMPRESSION_LEVEL.store(current - 1, Ordering::Relaxed);
                                }
                            }
                            KeyCode::Char(']') => {
                                let current = crate::COMPRESSION_LEVEL.load(Ordering::Relaxed);
                                if current < 9 {
                                    crate::COMPRESSION_LEVEL.store(current + 1, Ordering::Relaxed);
                                }
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
            state_guard.level = crate::COMPRESSION_LEVEL.load(Ordering::Relaxed);

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

fn render_history_chart(history: &[f64], height: usize, data_width: usize) -> Vec<String> {
    let mut chart_lines = vec![String::new(); height];
    let max_val = history.iter().copied().fold(0.0, f64::max);
    let scale = if max_val > 0.0 { max_val } else { 1.0 };

    let (top_label, bot_label) = if max_val >= 1024.0 * 1024.0 * 1024.0 {
        (
            format!("{:5.1}G ┼", max_val / (1024.0 * 1024.0 * 1024.0)),
            "  0.0G ┼".to_string(),
        )
    } else if max_val >= 1024.0 * 1024.0 {
        (
            format!("{:5.1}M ┼", max_val / (1024.0 * 1024.0)),
            "  0.0M ┼".to_string(),
        )
    } else if max_val >= 1024.0 {
        (
            format!("{:5.1}K ┼", max_val / 1024.0),
            "  0.0K ┼".to_string(),
        )
    } else {
        (
            format!("{:5.0}B ┼", max_val),
            "    0B ┼".to_string(),
        )
    };
    let mid_label = "       │".to_string();

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
        "PAUSED"
    } else if state.is_complete {
        "COMPLETE"
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

        // Nested Grid Layout
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),  // Title Row
                Constraint::Length(12), // Body panels (Left: Status / Right: Speed Chart)
                Constraint::Length(6),  // Logs (5 total height, 4 logs inner)
                Constraint::Length(3),  // Footer Controls
            ])
            .split(rect);

        // --- 1. Title / Header Row ---
        let status_color = match pause_status {
            "PAUSED" => Color::Red,
            "COMPLETE" => Color::Green,
            _ => Color::Indexed(117), // Cyan-blue
        };
        let title_line = Line::from(vec![
            Span::styled("ZIPMT PIPELINE CONTROLLER ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled("═".repeat(38), style_orange),
            Span::styled(format!(" [STATUS: {}]", pause_status), Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
        ]);
        f.render_widget(Paragraph::new(title_line), chunks[0]);

        // --- 2. Body Panels ---
        let body_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(41), // Left Panel: width 41
                Constraint::Length(39), // Right Panel: width 39
            ])
            .split(chunks[1]);

        // Left Panel Block
        let left_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(style_orange)
            .title(Span::styled(" Transporter Status ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));

        // Right Panel Block
        let right_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(style_orange)
            .title(Span::styled(" Ingest Speed History ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));

        // Render Left Panel Content
        match state.mode {
            TuiMode::Split => {
                let mut lines = Vec::new();
                for stripe in &state.stripes {
                    let total = stripe.total_bytes;
                    let processed = stripe.bytes_processed;
                    let pct = if total > 0 { (processed * 100) / total } else { 0 };
                    
                    let ratio = if stripe.bytes_written > 0 {
                        processed as f64 / stripe.bytes_written as f64
                    } else {
                        1.0
                    };
                    
                    let bar_len = 15;
                    let filled = if total > 0 { std::cmp::min((processed * bar_len) / total, bar_len) } else { 0 };
                    let bar: String = std::iter::repeat('█')
                        .take(filled)
                        .chain(std::iter::repeat('░').take(bar_len - filled))
                        .collect();
                        
                    lines.push(Line::from(vec![
                        Span::styled(format!("Sec {:02}: ", stripe.id), style_purple),
                        Span::styled(format!("[{}] ", bar), style_cyan),
                        Span::styled(format!("{:3}% ", pct), style_yellow),
                        Span::styled(format!("({:.1}x)", ratio), style_cyan),
                    ]));
                }
                while lines.len() < 10 {
                    lines.push(Line::from(""));
                }
                f.render_widget(Paragraph::new(lines).block(left_block), body_chunks[0]);
            }
            TuiMode::Stream => {
                let speed_in = if elapsed > 0.0 {
                    state.bytes_read as f64 / elapsed
                } else {
                    0.0
                };

                let ratio = if state.bytes_written > 0 {
                    state.bytes_read as f64 / state.bytes_written as f64
                } else {
                    1.0
                };

                let mut lines = Vec::new();
                lines.push(Line::from(vec![
                    Span::styled("Ingested : ", style_purple),
                    Span::styled(format!("{:7.2}", state.bytes_read as f64 / (1024.0 * 1024.0)), style_cyan),
                    Span::styled(" MB / ", style_purple),
                    Span::styled(format!("{:5.1}", speed_in / (1024.0 * 1024.0)), style_cyan),
                    Span::styled(" MB/s", style_purple),
                ]));
                lines.push(Line::from(vec![
                    Span::styled("Output   : ", style_purple),
                    Span::styled(format!("{:7.2}", state.bytes_written as f64 / (1024.0 * 1024.0)), style_cyan),
                    Span::styled(" MB (", style_purple),
                    Span::styled(format!("{:.2}x", ratio), style_yellow),
                    Span::styled(" Ratio)", style_purple),
                ]));
                lines.push(Line::from(Span::styled("─".repeat(39), style_orange)));

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
                lines.push(Line::from(vec![
                    Span::styled("Queue: ", style_purple),
                    Span::styled(queue_list, style_cyan),
                    Span::styled(format!(" ({}/{} blk)", state.queue_depth, state.queue_capacity), style_yellow),
                ]));

                let mut w_parts_1 = Vec::new();
                if state.workers.len() > 0 {
                    let w = &state.workers[0];
                    w_parts_1.push(Span::styled(format!("W00:[{:<4}] ", w.status), style_purple));
                    w_parts_1.push(Span::styled(w.current_chunk.map(|c| format!("#{:<3}", c)).unwrap_or_else(|| "-- ".to_string()), style_cyan));
                }
                if state.workers.len() > 1 {
                    w_parts_1.push(Span::styled("   ", style_purple));
                    let w = &state.workers[1];
                    w_parts_1.push(Span::styled(format!("W01:[{:<4}] ", w.status), style_purple));
                    w_parts_1.push(Span::styled(w.current_chunk.map(|c| format!("#{:<3}", c)).unwrap_or_else(|| "-- ".to_string()), style_cyan));
                }
                lines.push(Line::from(w_parts_1));

                let mut w_parts_2 = Vec::new();
                if state.workers.len() > 2 {
                    let w = &state.workers[2];
                    w_parts_2.push(Span::styled(format!("W02:[{:<4}] ", w.status), style_purple));
                    w_parts_2.push(Span::styled(w.current_chunk.map(|c| format!("#{:<3}", c)).unwrap_or_else(|| "-- ".to_string()), style_cyan));
                }
                if state.workers.len() > 3 {
                    w_parts_2.push(Span::styled("   ", style_purple));
                    let w = &state.workers[3];
                    w_parts_2.push(Span::styled(format!("W03:[{:<4}] ", w.status), style_purple));
                    w_parts_2.push(Span::styled(w.current_chunk.map(|c| format!("#{:<3}", c)).unwrap_or_else(|| "-- ".to_string()), style_cyan));
                }
                lines.push(Line::from(w_parts_2));

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
                lines.push(Line::from(vec![
                    Span::styled("Out Q: ", style_purple),
                    Span::styled(out_queue_list, style_cyan),
                    Span::styled(format!(" (Next: #{})", state.next_expected_seq), style_yellow),
                ]));

                let gap_str = if !state.output_buffer.is_empty() && state.output_buffer[0] > state.next_expected_seq {
                    format!("GAP AT #{}", state.next_expected_seq)
                } else {
                    "NO GAPS".to_string()
                };
                let gap_style = if gap_str.starts_with("GAP") {
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Green)
                };
                lines.push(Line::from(vec![
                    Span::styled("Gaps : ", style_purple),
                    Span::styled(gap_str, gap_style),
                ]));

                let proj_1m = speed_in * 60.0;
                let proj_5m = speed_in * 300.0;
                lines.push(Line::from(vec![
                    Span::styled("Target 1m: ", style_purple),
                    Span::styled(format!("{:5.1}M", proj_1m / (1024.0 * 1024.0)), style_cyan),
                    Span::styled("  5m: ", style_purple),
                    Span::styled(format!("{:5.1}M", proj_5m / (1024.0 * 1024.0)), style_cyan),
                ]));

                while lines.len() < 10 {
                    lines.push(Line::from(""));
                }
                f.render_widget(Paragraph::new(lines).block(left_block), body_chunks[0]);
            }
        }

        // Render Right Panel Content (Speed Chart)
        let chart_lines = render_history_chart(&state.speed_history, 10, 29);
        let chart_paragraph = Paragraph::new(
            chart_lines.into_iter().map(|l| Line::from(Span::styled(l, style_yellow))).collect::<Vec<_>>()
        ).block(right_block);
        f.render_widget(chart_paragraph, body_chunks[1]);

        // --- 3. Logs Section ---
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
        let avg_time = state.get_avg_chunk_time_ms();
        let avg_str = if avg_time > 0.0 {
            format!("(avg:{:.1}ms)", avg_time)
        } else {
            "(avg:--)".to_string()
        };

        let logs_title = format!(
            " Log Messages (▲/▼) ── Knobs: P:{} C:{} Q:{} L:{} {} ",
            pool_size,
            chunk_size_str.replace("MB", "M"),
            qcap,
            state.level,
            avg_str
        );
        let logs_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(style_orange)
            .title(Span::styled(logs_title, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));

        let logs = if let Ok(buffer) = crate::get_log_buffer().lock() {
            buffer.clone()
        } else {
            Vec::new()
        };

        let scroll_offset = crate::LOG_SCROLL_OFFSET.load(Ordering::Relaxed);
        let max_offset = logs.len().saturating_sub(4); // 4 rows inner height
        let offset = std::cmp::min(scroll_offset, max_offset);
        crate::LOG_SCROLL_OFFSET.store(offset, Ordering::Relaxed);

        let mut log_lines = Vec::new();
        for r in 0..4 {
            if offset + r < logs.len() {
                let line_str = &logs[offset + r];
                let truncated = if line_str.len() > 76 {
                    format!("{}...", &line_str[..73])
                } else {
                    line_str.clone()
                };
                log_lines.push(Line::from(Span::styled(truncated, style_cyan)));
            } else {
                log_lines.push(Line::from(""));
            }
        }
        f.render_widget(Paragraph::new(log_lines).block(logs_block), chunks[2]);

        // --- 4. Footer Controls ---
        let footer_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(style_orange)
            .title(Span::styled(" Pipeline Controls ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));

        let filled_slider = ((throttle_delay as f64 / 500.0) * 8.0).round() as usize;
        let slider_bar: String = std::iter::repeat('█').take(filled_slider)
            .chain(std::iter::repeat('░').take(8 - filled_slider))
            .collect();

        let left_footer = Span::styled("CTRL: [P]Pause [-/+]Slow/Speed [[]/[]]Lvl-/+ [Q]Abort", style_purple);
        let right_footer = format!("  Throt:[{}] {:3}ms", slider_bar, throttle_delay);

        let footer_line = Line::from(vec![
            left_footer,
            Span::styled(right_footer, style_yellow),
        ]);
        f.render_widget(Paragraph::new(footer_line).block(footer_block), chunks[3]);
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
        let mut row_3_str = String::new();
        for x in 0..80 {
            row_3_str.push_str(buffer.get(x, 2).symbol());
        }
        assert!(
            row_3_str.contains("╭"),
            "Row 3 (0-indexed 2) must contain the top border ╭, got: {:?}",
            row_3_str
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
        let mut row_11_str = String::new();
        for x in 0..120 {
            row_11_str.push_str(buffer.get(x, 10).symbol());
        }
        assert!(
            row_11_str[20..].starts_with("╭"),
            "Row 11 (0-indexed 10) starting at col 20 must contain the top border ╭, got: {:?}",
            row_11_str
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
