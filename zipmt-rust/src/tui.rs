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
    // Rolling speed history (max 30 values)
    pub speed_history: Vec<f64>,
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

pub fn start_tui_thread(state: Arc<Mutex<TuiState>>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let guard = TerminalGuard::new();
        
        loop {
            // Listen for keypress events
            if event::poll(std::time::Duration::from_millis(0)).unwrap() {
                if let Event::Key(key) = event::read().unwrap() {
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
                    if state_guard.speed_history.len() > 30 {
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
            *line = " ".repeat(30);
        }
        return chart_lines;
    }

    let max_val = history.iter().copied().fold(0.0, f64::max);
    let scale = if max_val > 0.0 { max_val } else { 1.0 };

    for r in 0..height {
        let mut row_str = String::new();
        let padding = 30usize.saturating_sub(history.len());
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
            lines.push(format!("{}┌─── LCARS COMMAND PANEL ─ [ SYSTEM: ACTIVE ] ──────────────────────────┐{}", orange, reset));

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

            lines.push(format!(
                "{}│ {}Ingested : {}{:7.2} MB / {:7.2} MB    {}│ {}Speed: {}{:5.1} MB/s                     {}│{}",
                orange,
                cyan,
                yellow,
                total_in as f64 / (1024.0 * 1024.0),
                state.total_input_size as f64 / (1024.0 * 1024.0),
                orange,
                cyan,
                yellow,
                speed / (1024.0 * 1024.0),
                orange,
                reset
            ));
            lines.push(format!(
                "{}│ {}Output   : {}{:7.2} MB ({:5.2}x Ratio)  {}│ {}Time : {}{:5.1}s (ETA: {:<12})     {}│{}",
                orange,
                cyan,
                yellow,
                total_out as f64 / (1024.0 * 1024.0),
                ratio,
                orange,
                cyan,
                yellow,
                elapsed,
                eta_str,
                orange,
                reset
            ));
            lines.push(format!("{}├───────────────────────────────────────┬───────────────────────────────┤{}", orange, reset));
            lines.push(format!(
                "{}│ {}STRIPE SECTORS PROGRESS             {}│ {}INGEST SPEED HISTORY (30s)    {}│{}",
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
                        "{}Sec {:02}: {}[{}] {}{:3.0}% {}({:4.1}x)",
                        cyan,
                        stripe.id,
                        purple,
                        bar,
                        yellow,
                        pct,
                        purple,
                        stripe_ratio
                    )
                } else {
                    " ".repeat(37)
                };

                let right_str = &chart_lines[r];

                lines.push(format!(
                    "{}│ {}{:<37} {}│ {}{} {}│{}",
                    orange,
                    left_str,
                    reset,
                    orange,
                    yellow,
                    right_str,
                    orange,
                    reset
                ));
            }

            lines.push(format!("{}├───────────────────────────────────────┼───────────────────────────────┤{}", orange, reset));
            lines.push(format!(
                "{}│ {}CONTROLS: [P] Pause  [-] Slow Down  {}│ {}STATUS: THROTTLE: {:3}ms    {}│{}",
                orange,
                purple,
                orange,
                purple,
                throttle_delay,
                orange,
                reset
            ));
            lines.push(format!(
                "{}│           [+] Speed Up  [Q] Abort     {}│         STATE   : {}     {}│{}",
                orange,
                orange,
                pause_status,
                orange,
                reset
            ));
            lines.push(format!("{}╰───────────────────────────────────────┴───────────────────────────────╯{}", orange, reset));
        }
        TuiMode::Stream => {
            lines.push(format!("{}┌─── LCARS COMMAND PANEL ─ [ SYSTEM: ACTIVE ] ──────────────────────────┐{}", orange, reset));

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

            lines.push(format!(
                "{}│ {}Ingested : {}{:7.2} MB                 {}│ {}Speed: {}{:5.1} MB/s                     {}│{}",
                orange,
                cyan,
                yellow,
                state.bytes_read as f64 / (1024.0 * 1024.0),
                orange,
                cyan,
                yellow,
                speed_in / (1024.0 * 1024.0),
                orange,
                reset
            ));
            lines.push(format!(
                "{}│ {}Output   : {}{:7.2} MB ({:5.2}x Ratio)  {}│ {}Speed: {}{:5.1} MB/s                     {}│{}",
                orange,
                cyan,
                yellow,
                state.bytes_written as f64 / (1024.0 * 1024.0),
                ratio,
                orange,
                cyan,
                yellow,
                speed_out / (1024.0 * 1024.0),
                orange,
                reset
            ));
            lines.push(format!("{}├───────────────────────────────────────┬───────────────────────────────┤{}", orange, reset));
            lines.push(format!(
                "{}│ {}TRANSPORTER BUFFER CAPACITY          {}│ {}INGEST SPEED HISTORY (30s)    {}│{}",
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
                        "{}Buffer: {}[{}] {}{:2}/{:2} blk  ",
                        cyan,
                        purple,
                        bar,
                        yellow,
                        depth,
                        cap
                    )
                } else if r == 2 {
                    format!("{}1m Ingest Target : {}{:8.1} MB  ", cyan, yellow, proj_1m / (1024.0 * 1024.0))
                } else if r == 3 {
                    format!("{}5m Ingest Target : {}{:8.1} MB  ", cyan, yellow, proj_5m / (1024.0 * 1024.0))
                } else if r == 4 {
                    format!("{}10m Ingest Target: {}{:8.1} MB  ", cyan, yellow, proj_10m / (1024.0 * 1024.0))
                } else {
                    " ".repeat(37)
                };

                let right_str = &chart_lines[r];

                lines.push(format!(
                    "{}│ {}{:<37} {}│ {}{} {}│{}",
                    orange,
                    left_str,
                    reset,
                    orange,
                    yellow,
                    right_str,
                    orange,
                    reset
                ));
            }

            lines.push(format!("{}├───────────────────────────────────────┼───────────────────────────────┤{}", orange, reset));
            lines.push(format!(
                "{}│ {}CONTROLS: [P] Pause  [-] Slow Down  {}│ {}STATUS: THROTTLE: {:3}ms    {}│{}",
                orange,
                purple,
                orange,
                purple,
                throttle_delay,
                orange,
                reset
            ));
            lines.push(format!(
                "{}│           [+] Speed Up  [Q] Abort     {}│         STATE   : {}     {}│{}",
                orange,
                orange,
                pause_status,
                orange,
                reset
            ));
            lines.push(format!("{}╰───────────────────────────────────────┴───────────────────────────────╯{}", orange, reset));
        }
    }

    // Centering calculation
    #[cfg(test)]
    let (cols, rows) = (80, 24);
    #[cfg(not(test))]
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));

    let pad_left = (cols as usize).saturating_sub(78) / 2;
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
}
