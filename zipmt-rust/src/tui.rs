use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Instant;

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
        }
    }
}

pub fn start_tui_thread(state: Arc<Mutex<TuiState>>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        // Clear terminal screen and hide cursor
        eprint!("\x1B[2J\x1B[H\x1B[?25l");
        
        loop {
            let is_complete = {
                let guard = state.lock().unwrap();
                let mut stderr = std::io::stderr();
                draw_tui(&guard, &mut stderr);
                let _ = stderr.flush();
                guard.is_complete
            };

            if is_complete {
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        // Show cursor
        eprint!("\x1B[?25h");
    })
}

pub fn draw_tui(state: &TuiState, target: &mut dyn std::io::Write) {
    // Reset cursor to home position
    let _ = write!(target, "\x1B[H");

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

    match state.mode {
        TuiMode::Split => {
            let _ = writeln!(target, "{}╭────────────────────────────────────────────────────────────────────────╮{}", orange, reset);
            let _ = writeln!(target, "{}│ {}[LCARS-982] SYSTEM DIAGNOSTICS                                         {}│{}", orange, purple, orange, reset);
            let _ = writeln!(target, "{}├────────────────────────────────────────────────────────────────────────┤{}", orange, reset);

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

            let _ = writeln!(
                target,
                "{}│ {}Ingested : {}{:7.2} MB / {:7.2} MB {}| Speed: {}{:5.1} MB/s                     {}│{}",
                orange,
                cyan,
                yellow,
                total_in as f64 / (1024.0 * 1024.0),
                state.total_input_size as f64 / (1024.0 * 1024.0),
                cyan,
                yellow,
                speed / (1024.0 * 1024.0),
                orange,
                reset
            );
            let _ = writeln!(
                target,
                "{}│ {}Output   : {}{:7.2} MB             {}| Ratio: {}{:5.2}x                      {}│{}",
                orange,
                cyan,
                yellow,
                total_out as f64 / (1024.0 * 1024.0),
                cyan,
                yellow,
                ratio,
                orange,
                reset
            );
            let _ = writeln!(
                target,
                "{}│ {}Time     : {}{:5.1}s                 {}| ETA  : {}{:<12}                  {}│{}",
                orange,
                cyan,
                yellow,
                elapsed,
                cyan,
                yellow,
                eta_str,
                orange,
                reset
            );
            let _ = writeln!(target, "{}├────────────────────────────────────────────────────────────────────────┤{}", orange, reset);
            let _ = writeln!(target, "{}│ {}[STRIPE SECTORS] PROGRESS                                              {}│{}", orange, purple, orange, reset);

            for stripe in &state.stripes {
                let pct = if stripe.total_bytes > 0 {
                    (stripe.bytes_processed as f64 / stripe.total_bytes as f64) * 100.0
                } else {
                    0.0
                };
                
                let bar_len = 20;
                let filled = ((pct / 100.0) * bar_len as f64) as usize;
                let bar: String = std::iter::repeat('=')
                    .take(filled)
                    .chain(std::iter::repeat(' ').take(bar_len - filled))
                    .collect();

                let stripe_ratio = if stripe.bytes_written > 0 {
                    stripe.bytes_processed as f64 / stripe.bytes_written as f64
                } else {
                    1.0
                };

                let _ = writeln!(
                    target,
                    "{}│ {}Sector {:02}: {}[{}] {}{:3.0}% {}| In: {}{:7.1}KB {}| Out: {}{:7.1}KB {}({:.2}x) {}│{}",
                    orange,
                    cyan,
                    stripe.id,
                    purple,
                    bar,
                    yellow,
                    pct,
                    cyan,
                    yellow,
                    stripe.bytes_processed as f64 / 1024.0,
                    cyan,
                    yellow,
                    stripe.bytes_written as f64 / 1024.0,
                    purple,
                    stripe_ratio,
                    orange,
                    reset
                );
            }
            let _ = writeln!(target, "{}╰────────────────────────────────────────────────────────────────────────╯{}", orange, reset);
        }
        TuiMode::Stream => {
            let _ = writeln!(target, "{}╭────────────────────────────────────────────────────────────────────────╮{}", orange, reset);
            let _ = writeln!(target, "{}│ {}[LCARS-982] PIPELINE STREAM DIAGNOSTICS                                {}│{}", orange, purple, orange, reset);
            let _ = writeln!(target, "{}├────────────────────────────────────────────────────────────────────────┤{}", orange, reset);

            let cap = state.queue_capacity;
            let depth = state.queue_depth;
            let cap_f = if cap > 0 { cap } else { 1 };
            let bar_len = 20;
            let filled = (depth * bar_len) / cap_f;
            let bar: String = std::iter::repeat('=')
                .take(filled)
                .chain(std::iter::repeat(' ').take(bar_len - filled))
                .collect();

            let _ = writeln!(
                target,
                "{}│ {}Transporter Buffer: {}[{}] {}{}/{} blocks                             {}│{}",
                orange,
                cyan,
                purple,
                bar,
                yellow,
                depth,
                cap,
                orange,
                reset
            );
            let _ = writeln!(target, "{}├────────────────────────────────────────────────────────────────────────┤{}", orange, reset);

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

            let _ = writeln!(
                target,
                "{}│ {}Ingested : {}{:7.2} MB             {}| Speed: {}{:5.1} MB/s                     {}│{}",
                orange,
                cyan,
                yellow,
                state.bytes_read as f64 / (1024.0 * 1024.0),
                cyan,
                yellow,
                speed_in / (1024.0 * 1024.0),
                orange,
                reset
            );
            let _ = writeln!(
                target,
                "{}│ {}Output   : {}{:7.2} MB             {}| Speed: {}{:5.1} MB/s                     {}│{}",
                orange,
                cyan,
                yellow,
                state.bytes_written as f64 / (1024.0 * 1024.0),
                cyan,
                yellow,
                speed_out / (1024.0 * 1024.0),
                orange,
                reset
            );
            let _ = writeln!(
                target,
                "{}│ {}Ratio    : {}{:5.2}x                  {}| Time : {}{:5.1}s                         {}│{}",
                orange,
                cyan,
                yellow,
                ratio,
                cyan,
                yellow,
                elapsed,
                orange,
                reset
            );

            if state.total_input_size > 0 {
                let eta_str = if state.bytes_read == 0 {
                    "Estimating...".to_string()
                } else if state.bytes_read >= state.total_input_size {
                    "0s (Complete)".to_string()
                } else if speed_in > 0.0 {
                    let remaining = state.total_input_size.saturating_sub(state.bytes_read);
                    let eta_secs = (remaining as f64 / speed_in).round() as usize;
                    format!("{}s", eta_secs)
                } else {
                    "--".to_string()
                };
                let _ = writeln!(
                    target,
                    "{}│ {}Total    : {}{:7.2} MB             {}| ETA  : {}{:<12}                  {}│{}",
                    orange,
                    cyan,
                    yellow,
                    state.total_input_size as f64 / (1024.0 * 1024.0),
                    cyan,
                    yellow,
                    eta_str,
                    orange,
                    reset
                );
            }

            let _ = writeln!(target, "{}├────────────────────────────────────────────────────────────────────────┤{}", orange, reset);
            let _ = writeln!(target, "{}│ {}[CAPACITY FORECAST]                                                    {}│{}", orange, purple, orange, reset);
            let _ = writeln!(
                target,
                "{}│ {}  - 1-Minute Target : {}{:8.2} MB                                     {}│{}",
                orange,
                cyan,
                yellow,
                proj_1m / (1024.0 * 1024.0),
                orange,
                reset
            );
            let _ = writeln!(
                target,
                "{}│ {}  - 5-Minute Target : {}{:8.2} MB                                     {}│{}",
                orange,
                cyan,
                yellow,
                proj_5m / (1024.0 * 1024.0),
                orange,
                reset
            );
            let _ = writeln!(
                target,
                "{}│ {}  - 10-Minute Target: {}{:8.2} MB                                     {}│{}",
                orange,
                cyan,
                yellow,
                proj_10m / (1024.0 * 1024.0),
                orange,
                reset
            );
            let _ = writeln!(target, "{}╰────────────────────────────────────────────────────────────────────────╯{}", orange, reset);
        }
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
        // Pre-populate split mode with 400KB total input size
        let mut state = TuiState::new_split(4, 400 * 1024);
        
        state.stripes[0].total_bytes = 102400;
        state.stripes[0].bytes_processed = 102400; // 100%
        state.stripes[0].bytes_written = 40960;

        state.stripes[1].total_bytes = 102400;
        state.stripes[1].bytes_processed = 51200; // 50%
        state.stripes[1].bytes_written = 20480;

        state.stripes[2].total_bytes = 102400;
        state.stripes[2].bytes_processed = 0; // 0%
        state.stripes[2].bytes_written = 0;

        state.stripes[3].total_bytes = 102400;
        state.stripes[3].bytes_processed = 25600; // 25%
        state.stripes[3].bytes_written = 10240;

        let mut buf = Vec::new();
        draw_tui(&state, &mut buf);
        let output = String::from_utf8(buf).unwrap();
        let clean_output = strip_ansi(&output);

        insta::assert_snapshot!(clean_output);
    }

    #[test]
    fn test_tui_layout_stream_mode_snapshot() {
        let mut state = TuiState::new_stream(8, 0);
        state.bytes_read = 50 * 1024 * 1024; // 50MB
        state.bytes_written = 20 * 1024 * 1024; // 20MB
        state.queue_depth = 3;

        let mut buf = Vec::new();
        draw_tui(&state, &mut buf);
        let output = String::from_utf8(buf).unwrap();
        let clean_output = strip_ansi(&output);

        insta::assert_snapshot!(clean_output);
    }
}
