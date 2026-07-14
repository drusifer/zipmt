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
    // Split mode progress
    pub stripes: Vec<StripeProgress>,
    // Stream mode progress
    pub bytes_read: usize,
    pub bytes_written: usize,
    pub queue_depth: usize,
    pub queue_capacity: usize,
}

impl TuiState {
    pub fn new_split(total_stripes: usize) -> Self {
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
            stripes,
            bytes_read: 0,
            bytes_written: 0,
            queue_depth: 0,
            queue_capacity: 0,
        }
    }

    pub fn new_stream(queue_capacity: usize) -> Self {
        TuiState {
            mode: TuiMode::Stream,
            start_time: Instant::now(),
            is_complete: false,
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

    match state.mode {
        TuiMode::Split => {
            let _ = writeln!(target, "=== [zipmt-rust] Concurrency Progress (Split Mode) ===");
            let mut total_in = 0;
            let mut total_out = 0;

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

                let ratio = if stripe.bytes_written > 0 {
                    stripe.bytes_processed as f64 / stripe.bytes_written as f64
                } else {
                    1.0
                };

                let _ = writeln!(
                    target,
                    "Stripe {:2}: [{}] {:3.0}% | In: {:7.2}KB, Out: {:7.2}KB ({:.2}x)",
                    stripe.id,
                    bar,
                    pct,
                    stripe.bytes_processed as f64 / 1024.0,
                    stripe.bytes_written as f64 / 1024.0,
                    ratio
                );

                total_in += stripe.bytes_processed;
                total_out += stripe.bytes_written;
            }

            let speed = if elapsed > 0.0 {
                (total_in as f64 / (1024.0 * 1024.0)) / elapsed
            } else {
                0.0
            };

            let total_ratio = if total_out > 0 {
                total_in as f64 / total_out as f64
            } else {
                1.0
            };

            let _ = writeln!(target, "--------------------------------------------------");
            let _ = writeln!(
                target,
                "Total In: {:.2}MB | Out: {:.2}MB | Speed: {:.1}MB/s | Ratio: {:.2}x",
                total_in as f64 / (1024.0 * 1024.0),
                total_out as f64 / (1024.0 * 1024.0),
                speed,
                total_ratio
            );
        }
        TuiMode::Stream => {
            let _ = writeln!(target, "=== [zipmt-rust] Pipeline Stream Progress (Stream Mode) ===");

            let cap = state.queue_capacity;
            let depth = state.queue_depth;
            let cap_f = if cap > 0 { cap } else { 1 };
            let bar_len = 20;
            let filled = (depth * bar_len) / cap_f;
            let bar: String = std::iter::repeat('=')
                .take(filled)
                .chain(std::iter::repeat(' ').take(bar_len - filled))
                .collect();

            let _ = writeln!(target, "Queue Capacity: [{}] {}/{} blocks in transit", bar, depth, cap);

            let speed_in = if elapsed > 0.0 {
                (state.bytes_read as f64 / (1024.0 * 1024.0)) / elapsed
            } else {
                0.0
            };
            let speed_out = if elapsed > 0.0 {
                (state.bytes_written as f64 / (1024.0 * 1024.0)) / elapsed
            } else {
                0.0
            };

            let ratio = if state.bytes_written > 0 {
                state.bytes_read as f64 / state.bytes_written as f64
            } else {
                1.0
            };

            let _ = writeln!(target, "--------------------------------------------------");
            let _ = writeln!(target, "Throughput:");
            let _ = writeln!(target, "  - Read : {:7.2}MB ({:5.2}MB/s)", state.bytes_read as f64 / (1024.0 * 1024.0), speed_in);
            let _ = writeln!(target, "  - Write: {:7.2}MB ({:5.2}MB/s)", state.bytes_written as f64 / (1024.0 * 1024.0), speed_out);
            let _ = writeln!(target, "Compression Ratio: {:.2}x", ratio);
            let _ = writeln!(target, "Elapsed Time: {:.1}s", elapsed);
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
        let mut state = TuiState::new_split(4);
        // Pre-populate some progress values
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
        let mut state = TuiState::new_stream(8);
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
