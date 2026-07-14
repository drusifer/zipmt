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
                draw_tui(&guard);
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

fn draw_tui(state: &TuiState) {
    // Reset cursor to home position
    eprint!("\x1B[H");

    let elapsed = state.start_time.elapsed().as_secs_f64();

    match state.mode {
        TuiMode::Split => {
            eprintln!("=== [zipmt-rust] Concurrency Progress (Split Mode) ===");
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

                eprintln!(
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

            eprintln!("--------------------------------------------------");
            eprintln!(
                "Total In: {:.2}MB | Out: {:.2}MB | Speed: {:.1}MB/s | Ratio: {:.2}x",
                total_in as f64 / (1024.0 * 1024.0),
                total_out as f64 / (1024.0 * 1024.0),
                speed,
                total_ratio
            );
        }
        TuiMode::Stream => {
            eprintln!("=== [zipmt-rust] Pipeline Stream Progress (Stream Mode) ===");

            let cap = state.queue_capacity;
            let depth = state.queue_depth;
            let cap_f = if cap > 0 { cap } else { 1 };
            let bar_len = 20;
            let filled = (depth * bar_len) / cap_f;
            let bar: String = std::iter::repeat('=')
                .take(filled)
                .chain(std::iter::repeat(' ').take(bar_len - filled))
                .collect();

            eprintln!("Queue Capacity: [{}] {}/{} blocks in transit", bar, depth, cap);

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

            eprintln!("--------------------------------------------------");
            eprintln!("Throughput:");
            eprintln!("  - Read : {:7.2}MB ({:5.2}MB/s)", state.bytes_read as f64 / (1024.0 * 1024.0), speed_in);
            eprintln!("  - Write: {:7.2}MB ({:5.2}MB/s)", state.bytes_written as f64 / (1024.0 * 1024.0), speed_out);
            eprintln!("Compression Ratio: {:.2}x", ratio);
            eprintln!("Elapsed Time: {:.1}s", elapsed);
        }
    }
}
