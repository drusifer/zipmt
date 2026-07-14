use std::io::Write;
use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering;
use std::time::Instant;

use crossterm::event::{self, Event, KeyCode};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};

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

struct TerminalGuard;
impl TerminalGuard {
    pub fn new() -> Self {
        let _ = enable_raw_mode();
        let mut stderr = std::io::stderr();
        let _ = crossterm::queue!(stderr, EnterAlternateScreen, crossterm::cursor::Hide);
        let _ = stderr.flush();
        TerminalGuard
    }
}
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let mut stderr = std::io::stderr();
        let _ = crossterm::queue!(stderr, crossterm::cursor::Show, LeaveAlternateScreen);
        let _ = stderr.flush();
        let _ = disable_raw_mode();
    }
}

fn query_initial_size() -> (u16, u16) {
    if let (Ok(cols_str), Ok(rows_str)) = (std::env::var("COLUMNS"), std::env::var("LINES")) {
        if let (Ok(cols), Ok(rows)) = (cols_str.parse::<u16>(), rows_str.parse::<u16>()) {
            return (cols, rows);
        }
    }

    // Try running tput cols and tput lines
    if let Ok(output_cols) = std::process::Command::new("tput").arg("cols").output() {
        if let Ok(output_lines) = std::process::Command::new("tput").arg("lines").output() {
            let cols_str = String::from_utf8_lossy(&output_cols.stdout).trim().to_string();
            let lines_str = String::from_utf8_lossy(&output_lines.stdout).trim().to_string();
            if let (Ok(cols), Ok(rows)) = (cols_str.parse::<u16>(), lines_str.parse::<u16>()) {
                return (cols, rows);
            }
        }
    }

    crossterm::terminal::size().unwrap_or((80, 24))
}

pub fn start_tui_thread(state: Arc<Mutex<TuiState>>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let guard = TerminalGuard::new();

        // Get initial terminal size
        let (w, h) = query_initial_size();
        {
            let mut guard = state.lock().unwrap();
            guard.terminal_cols = w;
            guard.terminal_rows = h;
        }
        
        loop {
            // Listen for terminal events
            if event::poll(std::time::Duration::from_millis(0)).unwrap() {
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

                let mut stderr = std::io::stderr();
                draw_tui(&state_guard, &mut stderr);
                let _ = stderr.flush();
                state_guard.is_complete
            };

            if is_complete {
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    })
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

pub fn draw_tui(state: &TuiState, target: &mut dyn std::io::Write) {
    let mut lines: Vec<String> = Vec::new();

    // Standardized/mockable elapsed time for layout consistency in tests
    #[cfg(test)]
    let elapsed = 1.234;
    #[cfg(not(test))]
    let elapsed = state.start_time.elapsed().as_secs_f64();

    // Style Colors: LCARS Palette
    let orange = "\x1B[38;5;208m";
    let purple = "\x1B[38;5;147m";
    let cyan = "\x1B[38;5;117m";
    let yellow = "\x1B[38;5;220m";
    let reset = "\x1B[0m";

    let pause_status = if crate::IS_PAUSED.load(Ordering::Relaxed) {
        "PAUSED "
    } else {
        "RUNNING"
    };
    let throttle_delay = crate::THROTTLE_DELAY_MS.load(Ordering::Relaxed);

    match state.mode {
        TuiMode::Split => {
            lines.push(format!("{}┌─── LCARS COMMAND PANEL ─ [ SYSTEM: ACTIVE ] ─────────────────────────────────┐{}", orange, reset));

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

            let left_str = format!(
                "Ingested : {:7.2} MB / {:7.2} MB    ",
                total_in as f64 / (1024.0 * 1024.0),
                state.total_input_size as f64 / (1024.0 * 1024.0)
            );
            let right_str = format!(
                "Speed: {:5.1} MB/s                  ",
                speed / (1024.0 * 1024.0)
            );
            lines.push(format!(
                "{}│ {}{}{} │ {}{}{} │{}",
                orange,
                cyan,
                left_str,
                orange,
                cyan,
                right_str,
                orange,
                reset
            ));

            let left_str_2 = format!(
                "Output   : {:7.2} MB ({:5.2}x Ratio)  ",
                total_out as f64 / (1024.0 * 1024.0),
                ratio
            );
            let right_str_2 = format!(
                "Time : {:5.1}s (ETA: {:<12})     ",
                elapsed,
                eta_str
            );
            lines.push(format!(
                "{}│ {}{}{} │ {}{}{} │{}",
                orange,
                cyan,
                left_str_2,
                orange,
                cyan,
                right_str_2,
                orange,
                reset
            ));

            lines.push(format!("{}├───────────────────────────────────────┬──────────────────────────────────────┤{}", orange, reset));
            lines.push(format!(
                "{}│ {}STRIPE SECTORS PROGRESS             {}│ {}INGEST SPEED HISTORY (35s)    {}│{}",
                orange,
                purple,
                orange,
                purple,
                orange,
                reset
            ));

            let chart_lines = render_history_chart(&state.speed_history, 6);
            for r in 0..6 {
                let left_str = if r < state.stripes.len() {
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
                    format!(
                        "Sec {:02}: [{}] {:3.0}% ({:4.1}x) ",
                        stripe.id,
                        bar,
                        pct,
                        stripe_ratio
                    )
                } else {
                    " ".repeat(38)
                };

                let right_str = &chart_lines[r];

                lines.push(format!(
                    "{}│ {}{}{} │ {}{} {}│{}",
                    orange,
                    cyan,
                    left_str,
                    orange,
                    yellow,
                    right_str,
                    orange,
                    reset
                ));
            }

            lines.push(format!("{}├───────────────────────────────────────┼──────────────────────────────────────┤{}", orange, reset));
            
            let control_left_1 = "CONTROLS: [P] Pause  [-] Slow Down    ";
            let control_right_1 = format!("STATUS: THROTTLE: {:3}ms            ", throttle_delay);
            lines.push(format!(
                "{}│ {}{}{} │ {}{}{} │{}",
                orange,
                purple,
                control_left_1,
                orange,
                purple,
                control_right_1,
                orange,
                reset
            ));

            let control_left_2 = "          [+] Speed Up  [Q] Abort     ";
            let control_right_2 = format!("        STATE   : {:7}           ", pause_status);
            lines.push(format!(
                "{}│ {}{}{} │ {}{}{} │{}",
                orange,
                orange,
                control_left_2,
                orange,
                orange,
                control_right_2,
                orange,
                reset
            ));

            lines.push(format!("{}╰───────────────────────────────────────┴──────────────────────────────────────╯{}", orange, reset));
        }
        TuiMode::Stream => {
            lines.push(format!("{}┌─── LCARS COMMAND PANEL ─ [ SYSTEM: ACTIVE ] ─────────────────────────────────┐{}", orange, reset));

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

            let left_str = format!(
                "Ingested : {:7.2} MB                 ",
                state.bytes_read as f64 / (1024.0 * 1024.0)
            );
            let right_str = format!(
                "Speed: {:5.1} MB/s                  ",
                speed_in / (1024.0 * 1024.0)
            );
            lines.push(format!(
                "{}│ {}{}{} │ {}{}{} │{}",
                orange,
                cyan,
                left_str,
                orange,
                cyan,
                right_str,
                orange,
                reset
            ));

            let left_str_2 = format!(
                "Output   : {:7.2} MB ({:5.2}x Ratio)  ",
                state.bytes_written as f64 / (1024.0 * 1024.0),
                ratio
            );
            let right_str_2 = format!(
                "Speed: {:5.1} MB/s                  ",
                speed_out / (1024.0 * 1024.0)
            );
            lines.push(format!(
                "{}│ {}{}{} │ {}{}{} │{}",
                orange,
                cyan,
                left_str_2,
                orange,
                cyan,
                right_str_2,
                orange,
                reset
            ));

            lines.push(format!("{}├───────────────────────────────────────┬──────────────────────────────────────┤{}", orange, reset));
            lines.push(format!(
                "{}│ {}TRANSPORTER BUFFER CAPACITY          {}│ {}INGEST SPEED HISTORY (35s)    {}│{}",
                orange,
                purple,
                orange,
                purple,
                orange,
                reset
            ));

            let chart_lines = render_history_chart(&state.speed_history, 6);
            for r in 0..6 {
                let left_str = if r == 0 {
                    let cap = state.queue_capacity;
                    let depth = state.queue_depth;
                    let cap_f = if cap > 0 { cap } else { 1 };
                    let bar_len = 15;
                    let filled = std::cmp::min((depth * bar_len) / cap_f, bar_len);
                    let bar: String = std::iter::repeat('█')
                        .take(filled)
                        .chain(std::iter::repeat('░').take(bar_len - filled))
                        .collect();
                    format!(
                        "Buffer: [{}] {:2}/{:2} blk   ",
                        bar,
                        depth,
                        cap
                    )
                } else if r == 2 {
                    format!("1m Ingest Target : {:8.1} MB        ", proj_1m / (1024.0 * 1024.0))
                } else if r == 3 {
                    format!("5m Ingest Target : {:8.1} MB        ", proj_5m / (1024.0 * 1024.0))
                } else if r == 4 {
                    format!("10m Ingest Target: {:8.1} MB        ", proj_10m / (1024.0 * 1024.0))
                } else {
                    " ".repeat(38)
                };

                let right_str = &chart_lines[r];

                lines.push(format!(
                    "{}│ {}{}{} │ {}{} {}│{}",
                    orange,
                    cyan,
                    left_str,
                    orange,
                    yellow,
                    right_str,
                    orange,
                    reset
                ));
            }

            lines.push(format!("{}├───────────────────────────────────────┼──────────────────────────────────────┤{}", orange, reset));
            
            let control_left_1 = "CONTROLS: [P] Pause  [-] Slow Down    ";
            let control_right_1 = format!("STATUS: THROTTLE: {:3}ms            ", throttle_delay);
            lines.push(format!(
                "{}│ {}{}{} │ {}{}{} │{}",
                orange,
                purple,
                control_left_1,
                orange,
                purple,
                control_right_1,
                orange,
                reset
            ));

            let control_left_2 = "          [+] Speed Up  [Q] Abort     ";
            let control_right_2 = format!("        STATE   : {:7}           ", pause_status);
            lines.push(format!(
                "{}│ {}{}{} │ {}{}{} │{}",
                orange,
                orange,
                control_left_2,
                orange,
                orange,
                control_right_2,
                orange,
                reset
            ));

            lines.push(format!("{}╰───────────────────────────────────────┴──────────────────────────────────────╯{}", orange, reset));
        }
    }

    let cols = state.terminal_cols;
    let rows = state.terminal_rows;

    let pad_left = (cols as usize).saturating_sub(80) / 2;
    let pad_top = (rows as usize).saturating_sub(16) / 2;

    // Reset cursor to home position and clear screen
    let _ = write!(target, "\x1B[H\x1B[2J");

    for _ in 0..pad_top {
        let _ = writeln!(target);
    }

    let padding = " ".repeat(pad_left);
    for line in lines {
        let _ = writeln!(target, "{}{}", padding, line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip_ansi(input: &str) -> String {
        let re = regex::Regex::new(r"\x1B\[[0-9;]*[a-zA-Z]").unwrap();
        re.replace_all(input, "").into_owned()
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

        let mut buf = Vec::new();
        draw_tui(&state, &mut buf);
        let output = String::from_utf8(buf).unwrap();
        let clean_output = strip_ansi(&output);

        insta::assert_snapshot!(clean_output);
    }

    #[test]
    fn test_tui_layout_stream_mode_snapshot() {
        let mut state = TuiState::new_stream(8, 0);
        state.bytes_read = 50 * 1024 * 1024;
        state.bytes_written = 20 * 1024 * 1024;
        state.queue_depth = 3;

        state.speed_history = vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];

        let mut buf = Vec::new();
        draw_tui(&state, &mut buf);
        let output = String::from_utf8(buf).unwrap();
        let clean_output = strip_ansi(&output);

        insta::assert_snapshot!(clean_output);
    }

    #[test]
    fn test_tui_layout_split_overflow() {
        // Test that if processed bytes exceed total_bytes, filled bar is capped and doesn't panic
        let mut state = TuiState::new_split(1, 100 * 1024);
        state.stripes[0].total_bytes = 100000;
        state.stripes[0].bytes_processed = 120000; // 120%
        state.stripes[0].bytes_written = 40000;

        let mut buf = Vec::new();
        // Should not panic due to index out of bounds or negative repeats
        draw_tui(&state, &mut buf);
        let output = String::from_utf8(buf).unwrap();
        let clean_output = strip_ansi(&output);
        assert!(clean_output.contains("120%"));
    }

    #[test]
    fn test_tui_layout_stream_overflow() {
        // Test that if queue_depth exceeds capacity, progress bar is capped and doesn't panic
        let mut state = TuiState::new_stream(8, 0);
        state.queue_depth = 12; // Exceeds cap (8)

        let mut buf = Vec::new();
        // Should not panic due to capacity overflow / subtraction underflow
        draw_tui(&state, &mut buf);
        let output = String::from_utf8(buf).unwrap();
        let clean_output = strip_ansi(&output);
        assert!(clean_output.contains("12/ 8 blk"));
    }
}
