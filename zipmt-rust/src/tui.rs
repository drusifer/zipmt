use std::collections::VecDeque;
use std::io::Write;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Axis, Block, BorderType, Borders, Chart, Clear, Dataset, Gauge, GraphType, Paragraph,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TuiMode {
    Split,
    Stream,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IoChartMode {
    Rate,
    Cumulative,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct IoSample {
    pub input_rate: f64,
    pub output_rate: f64,
    pub input_total: usize,
    pub output_total: usize,
}

const IO_BUCKET_INTERVAL: Duration = Duration::from_secs(1);

pub struct StripeProgress {
    pub id: usize,
    pub stage: crate::pipeline::SplitStage,
    pub bytes_processed: usize,
    pub total_bytes: usize,
    pub bytes_written: usize,
    pub started_at: Option<Instant>,
    pub completed_at: Option<Instant>,
}

impl StripeProgress {
    fn active_duration(&self, now: Instant) -> Option<Duration> {
        self.started_at.map(|started| {
            self.completed_at
                .unwrap_or(now)
                .saturating_duration_since(started)
        })
    }

    fn average_rates(&self, now: Instant) -> (f64, f64) {
        let seconds = self
            .active_duration(now)
            .map(|duration| duration.as_secs_f64())
            .unwrap_or(0.0);
        if seconds > 0.0 {
            (
                self.bytes_processed as f64 / seconds,
                self.bytes_written as f64 / seconds,
            )
        } else {
            (0.0, 0.0)
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SplitAggregate {
    pub input_processed: usize,
    pub input_total: usize,
    pub output_produced: usize,
    pub completed_input: usize,
    pub completed_output: usize,
    pub active_sectors: usize,
    pub completed_sectors: usize,
}

impl SplitAggregate {
    pub fn percent(self) -> f64 {
        if self.input_total == 0 {
            0.0
        } else {
            (self.input_processed as f64 / self.input_total as f64 * 100.0).min(100.0)
        }
    }

    pub fn ratio(self) -> Option<f64> {
        (self.completed_output > 0)
            .then(|| self.completed_input as f64 / self.completed_output as f64)
    }

    pub fn eta(self, input_rate: f64) -> Option<Duration> {
        (input_rate > 0.0).then(|| {
            Duration::from_secs_f64(
                self.input_total.saturating_sub(self.input_processed) as f64 / input_rate,
            )
        })
    }
}

fn composite_input_rate(aggregate: SplitAggregate, elapsed: Duration) -> f64 {
    let seconds = elapsed.as_secs_f64();
    if seconds > 0.0 {
        aggregate.input_processed as f64 / seconds
    } else {
        0.0
    }
}

fn composite_eta(aggregate: SplitAggregate, elapsed: Duration) -> Option<Duration> {
    aggregate.eta(composite_input_rate(aggregate, elapsed))
}

#[derive(Clone, Debug)]
pub struct WorkerState {
    pub id: usize,
    pub status: &'static str, // "IDLE", "BUSY", "HOLD"
    pub current_chunk: Option<u64>,
    pub display_chunk: Option<u64>,
    pub bytes_processed: usize,
    pub total_bytes: usize,
    pub bytes_written: usize,
    pub started_at: Option<Instant>,
    pub completed_at: Option<Instant>,
    pub ratio_history: VecDeque<f64>,
    pub input_rate_history: VecDeque<f64>,
}

const WORKER_AVERAGE_WINDOW: usize = 10;

impl WorkerState {
    fn assignment_input_rate(&self, now: Instant) -> f64 {
        let seconds = self
            .started_at
            .map(|started| {
                self.completed_at
                    .unwrap_or(now)
                    .saturating_duration_since(started)
                    .as_secs_f64()
            })
            .unwrap_or(0.0);
        if seconds > 0.0 {
            self.bytes_processed as f64 / seconds
        } else {
            0.0
        }
    }

    fn average_input_rate(&self, now: Instant) -> f64 {
        let live_rate = (self.completed_at.is_none() && self.bytes_processed > 0)
            .then(|| self.assignment_input_rate(now))
            .filter(|rate| *rate > 0.0);
        let history_to_use = WORKER_AVERAGE_WINDOW.saturating_sub(usize::from(live_rate.is_some()));
        let history = self
            .input_rate_history
            .iter()
            .rev()
            .take(history_to_use)
            .copied();
        let mut count = 0;
        let mut total = 0.0;
        for rate in history.chain(live_rate) {
            total += rate;
            count += 1;
        }
        if count > 0 { total / count as f64 } else { 0.0 }
    }

    fn eta(&self, now: Instant) -> Option<Duration> {
        let rate = self.average_input_rate(now);
        (rate > 0.0).then(|| {
            Duration::from_secs_f64(
                self.total_bytes.saturating_sub(self.bytes_processed) as f64 / rate,
            )
        })
    }

    fn record_final_metrics(&mut self, now: Instant) {
        if self.bytes_written == 0 {
            return;
        }
        if self.ratio_history.len() == WORKER_AVERAGE_WINDOW {
            self.ratio_history.pop_front();
        }
        self.ratio_history
            .push_back(self.bytes_processed as f64 / self.bytes_written as f64);
        let input_rate = self.assignment_input_rate(now);
        if input_rate > 0.0 {
            if self.input_rate_history.len() == WORKER_AVERAGE_WINDOW {
                self.input_rate_history.pop_front();
            }
            self.input_rate_history.push_back(input_rate);
        }
    }

    fn average_ratio(&self) -> Option<f64> {
        (!self.ratio_history.is_empty())
            .then(|| self.ratio_history.iter().sum::<f64>() / self.ratio_history.len() as f64)
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum FocusedWidget {
    None,
    CompressionLevelSlider,
    ThrottleDelaySlider,
    ChunkSizeSlider,
    WorkerCountSlider,
}

fn next_focus(current: FocusedWidget) -> FocusedWidget {
    match current {
        FocusedWidget::None => FocusedWidget::CompressionLevelSlider,
        FocusedWidget::CompressionLevelSlider => FocusedWidget::ThrottleDelaySlider,
        FocusedWidget::ThrottleDelaySlider => FocusedWidget::ChunkSizeSlider,
        FocusedWidget::ChunkSizeSlider => FocusedWidget::WorkerCountSlider,
        FocusedWidget::WorkerCountSlider => FocusedWidget::None,
    }
}

fn next_focus_for_mode(mode: TuiMode, current: FocusedWidget) -> FocusedWidget {
    if mode == TuiMode::Split {
        match current {
            FocusedWidget::ThrottleDelaySlider => FocusedWidget::ChunkSizeSlider,
            FocusedWidget::ChunkSizeSlider => FocusedWidget::None,
            _ => FocusedWidget::ThrottleDelaySlider,
        }
    } else {
        next_focus(current)
    }
}

fn slider_fraction(row: usize, height: usize) -> f64 {
    if height <= 1 {
        0.0
    } else {
        row.min(height - 1) as f64 / (height - 1) as f64
    }
}

fn chunk_size_from_slider_row(row: usize, height: usize) -> usize {
    let step = (slider_fraction(row, height) * 7.0).round() as u32;
    (8 * 1024 * 1024) >> step
}

fn workers_from_slider_row(row: usize, height: usize, max_workers: usize) -> usize {
    let max_workers = max_workers.max(1);
    let span = max_workers - 1;
    max_workers
        .saturating_sub((slider_fraction(row, height) * span as f64).round() as usize)
        .max(1)
}

fn footer_height_for_rows(rows: u16) -> u16 {
    6 + rows.saturating_sub(22).saturating_div(4).min(4)
}

fn log_window_start(log_count: usize, visible_rows: usize, lines_back: usize) -> usize {
    log_count
        .saturating_sub(visible_rows)
        .saturating_sub(lines_back.min(log_count.saturating_sub(visible_rows)))
}

fn split_page_size(rows: u16) -> usize {
    let body_height = 11 + rows.saturating_sub(22) / 2;
    body_height.saturating_sub(4).max(2) as usize / 2
}

fn parse_process_cpu_ticks(stat: &str) -> Option<u64> {
    let fields = stat.get(stat.rfind(')')? + 1..)?.split_whitespace();
    let fields = fields.collect::<Vec<_>>();
    Some(fields.get(11)?.parse::<u64>().ok()? + fields.get(12)?.parse::<u64>().ok()?)
}

fn parse_resident_memory(status: &str) -> Option<usize> {
    let line = status.lines().find(|line| line.starts_with("VmRSS:"))?;
    let kib = line.split_whitespace().nth(1)?.parse::<usize>().ok()?;
    Some(kib.saturating_mul(1024))
}

fn format_knob_rows(
    height: usize,
    top: &str,
    current: &str,
    bottom: &str,
    fraction: f64,
) -> Vec<String> {
    let height = height.max(3);
    let marker = ((1.0 - fraction.clamp(0.0, 1.0)) * (height - 1) as f64).round() as usize;
    (0..height)
        .map(|row| {
            let label = if row == 0 {
                top
            } else if row == marker {
                current
            } else if row == height - 1 {
                bottom
            } else {
                ""
            };
            let bar = if row >= marker { '█' } else { '░' };
            let pointer = if row == marker { '◀' } else { ' ' };
            format!("{:>7} {}{}", label, bar, pointer)
        })
        .collect()
}

pub struct TuiState {
    pub mode: TuiMode,
    pub start_time: Instant,
    pub is_complete: bool,
    pub final_elapsed: Option<Duration>,
    pub total_input_size: usize,
    // Split mode progress
    pub stripes: Vec<StripeProgress>,
    pub split_sector_offset: usize,
    pub split_final_bytes_written: usize,
    pub process_cpu_percent: Option<f64>,
    pub process_memory_bytes: Option<usize>,
    pub previous_cpu_ticks: Option<u64>,
    // Stream mode progress
    pub bytes_read: usize,
    pub bytes_written: usize,
    pub queue_depth: usize,
    pub queue_capacity: usize,
    // Rolling speed history (max 35 values)
    pub speed_history: Vec<f64>,
    pub io_history: Vec<IoSample>,
    pub io_chart_mode: IoChartMode,
    // Terminal dimensions
    pub terminal_cols: u16,
    pub terminal_rows: u16,
    // Rolling instant speed tracking
    pub prev_total_in: usize,
    pub prev_total_out: usize,
    pub last_speed_update: Instant,
    pub last_io_sample: Instant,
    // Queue / Worker / Gaps tracking
    pub input_queue: Vec<u64>,
    pub workers: Vec<WorkerState>,
    pub output_buffer: Vec<u64>,
    pub next_expected_seq: u64,
    // Compression level
    pub level: u32,
    pub total_chunks_compressed: u64,
    pub total_compress_time_ms: f64,

    // Decoupled focus and dynamic adjustment fields
    pub focused_widget: FocusedWidget,
    pub throttle_delay_ms: u64,
    pub chunk_size: usize,
    pub active_workers: usize,
    pub max_workers: usize,
    pub is_paused: bool,
}

impl TuiState {
    pub fn new_split(total_stripes: usize, total_input_size: usize, level: u32) -> Self {
        let mut stripes = Vec::new();
        for id in 0..total_stripes {
            stripes.push(StripeProgress {
                id,
                stage: crate::pipeline::SplitStage::Waiting,
                bytes_processed: 0,
                total_bytes: 0,
                bytes_written: 0,
                started_at: None,
                completed_at: None,
            });
        }
        TuiState {
            mode: TuiMode::Split,
            start_time: Instant::now(),
            is_complete: false,
            final_elapsed: None,
            total_input_size,
            stripes,
            split_sector_offset: 0,
            split_final_bytes_written: 0,
            process_cpu_percent: None,
            process_memory_bytes: None,
            previous_cpu_ticks: None,
            bytes_read: 0,
            bytes_written: 0,
            queue_depth: 0,
            queue_capacity: 0,
            speed_history: Vec::new(),
            io_history: Vec::new(),
            io_chart_mode: IoChartMode::Rate,
            terminal_cols: 80,
            terminal_rows: 24,
            prev_total_in: 0,
            prev_total_out: 0,
            last_speed_update: Instant::now(),
            last_io_sample: Instant::now(),
            input_queue: Vec::new(),
            workers: Vec::new(),
            output_buffer: Vec::new(),
            next_expected_seq: 0,
            level,
            total_chunks_compressed: 0,
            total_compress_time_ms: 0.0,
            focused_widget: FocusedWidget::None,
            throttle_delay_ms: 0,
            chunk_size: crate::pipeline::DEFAULT_CHUNK_SIZE,
            active_workers: total_stripes.max(1),
            max_workers: total_stripes.max(1),
            is_paused: false,
        }
    }

    pub fn new_stream(
        queue_capacity: usize,
        total_input_size: usize,
        num_workers: usize,
        level: u32,
    ) -> Self {
        let mut workers = Vec::new();
        for id in 0..num_workers {
            workers.push(WorkerState {
                id,
                status: "IDLE",
                current_chunk: None,
                display_chunk: None,
                bytes_processed: 0,
                total_bytes: 0,
                bytes_written: 0,
                started_at: None,
                completed_at: None,
                ratio_history: VecDeque::with_capacity(WORKER_AVERAGE_WINDOW),
                input_rate_history: VecDeque::with_capacity(WORKER_AVERAGE_WINDOW),
            });
        }
        TuiState {
            mode: TuiMode::Stream,
            start_time: Instant::now(),
            is_complete: false,
            final_elapsed: None,
            total_input_size,
            stripes: Vec::new(),
            split_sector_offset: 0,
            split_final_bytes_written: 0,
            process_cpu_percent: None,
            process_memory_bytes: None,
            previous_cpu_ticks: None,
            bytes_read: 0,
            bytes_written: 0,
            queue_depth: 0,
            queue_capacity,
            speed_history: Vec::new(),
            io_history: Vec::new(),
            io_chart_mode: IoChartMode::Rate,
            terminal_cols: 80,
            terminal_rows: 24,
            prev_total_in: 0,
            prev_total_out: 0,
            last_speed_update: Instant::now(),
            last_io_sample: Instant::now(),
            input_queue: Vec::new(),
            workers,
            output_buffer: Vec::new(),
            next_expected_seq: 0,
            level,
            total_chunks_compressed: 0,
            total_compress_time_ms: 0.0,
            focused_widget: FocusedWidget::None,
            throttle_delay_ms: 0,
            chunk_size: crate::pipeline::DEFAULT_CHUNK_SIZE,
            active_workers: num_workers.max(1),
            max_workers: num_workers.max(1),
            is_paused: false,
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

    pub fn toggle_io_chart_mode(&mut self) {
        self.io_chart_mode = match self.io_chart_mode {
            IoChartMode::Rate => IoChartMode::Cumulative,
            IoChartMode::Cumulative => IoChartMode::Rate,
        };
    }

    pub fn elapsed(&self) -> Duration {
        self.final_elapsed
            .unwrap_or_else(|| self.start_time.elapsed())
    }

    pub fn sample_io(&mut self, elapsed: Duration) {
        if elapsed.is_zero() {
            return;
        }
        let (input_total, output_total) = match self.mode {
            TuiMode::Split => {
                let aggregate = self.split_aggregate();
                (
                    aggregate.input_processed,
                    aggregate
                        .output_produced
                        .saturating_add(self.split_final_bytes_written),
                )
            }
            TuiMode::Stream => (self.bytes_read, self.bytes_written),
        };
        let seconds = elapsed.as_secs_f64();
        let input_delta = input_total.saturating_sub(self.prev_total_in);
        let output_delta = output_total.saturating_sub(self.prev_total_out);
        self.io_history.push(IoSample {
            input_rate: input_delta as f64 / seconds,
            output_rate: output_delta as f64 / seconds,
            input_total,
            output_total,
        });
        self.prev_total_in = input_total;
        self.prev_total_out = output_total;
        let capacity = (self.terminal_cols as usize).clamp(80, 512);
        if self.io_history.len() > capacity {
            self.io_history.drain(..self.io_history.len() - capacity);
        }
    }

    fn sample_io_bucket(&mut self, now: Instant) -> bool {
        let elapsed = now.duration_since(self.last_io_sample);
        if elapsed < IO_BUCKET_INTERVAL {
            return false;
        }
        self.sample_io(elapsed);
        self.last_io_sample = now;
        true
    }

    pub fn sample_system_metrics(&mut self, elapsed: Duration) {
        if elapsed.is_zero() {
            return;
        }
        let ticks = std::fs::read_to_string("/proc/self/stat")
            .ok()
            .and_then(|stat| parse_process_cpu_ticks(&stat));
        if let Some(current_ticks) = ticks {
            self.process_cpu_percent = self.previous_cpu_ticks.map(|previous| {
                current_ticks.saturating_sub(previous) as f64 / elapsed.as_secs_f64()
            });
            self.previous_cpu_ticks = Some(current_ticks);
        }
        self.process_memory_bytes = std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|status| parse_resident_memory(&status));
    }

    pub fn split_aggregate(&self) -> SplitAggregate {
        self.stripes
            .iter()
            .fold(SplitAggregate::default(), |mut aggregate, stripe| {
                let processed = stripe.bytes_processed.min(stripe.total_bytes);
                aggregate.input_processed = aggregate.input_processed.saturating_add(processed);
                aggregate.input_total = aggregate.input_total.saturating_add(stripe.total_bytes);
                aggregate.output_produced = aggregate
                    .output_produced
                    .saturating_add(stripe.bytes_written);
                match stripe.stage {
                    crate::pipeline::SplitStage::Waiting => {}
                    crate::pipeline::SplitStage::Running => aggregate.active_sectors += 1,
                    crate::pipeline::SplitStage::Done => {
                        aggregate.completed_sectors += 1;
                        aggregate.completed_input =
                            aggregate.completed_input.saturating_add(stripe.total_bytes);
                        aggregate.completed_output = aggregate
                            .completed_output
                            .saturating_add(stripe.bytes_written);
                    }
                }
                aggregate
            })
    }

    pub fn apply_progress_event(&mut self, event: crate::pipeline::ProgressEvent) {
        use crate::pipeline::ProgressEvent;

        match event {
            ProgressEvent::SplitProgress {
                stripe_id,
                stage,
                bytes_processed,
                bytes_written,
                total_bytes,
            } => {
                if let Some(stripe) = self.stripes.get_mut(stripe_id) {
                    let now = Instant::now();
                    stripe.stage = stage;
                    if stage == crate::pipeline::SplitStage::Running && stripe.started_at.is_none()
                    {
                        stripe.started_at = Some(now);
                    }
                    if stage == crate::pipeline::SplitStage::Done && stripe.completed_at.is_none() {
                        stripe.completed_at = Some(now);
                    }
                    stripe.bytes_processed = bytes_processed.min(total_bytes);
                    stripe.bytes_written = stripe.bytes_written.max(bytes_written);
                    stripe.total_bytes = total_bytes;
                }
            }
            ProgressEvent::SplitFinalWrite { bytes_written } => {
                self.split_final_bytes_written = self.split_final_bytes_written.max(bytes_written);
            }
            ProgressEvent::StreamProgress {
                bytes_read,
                bytes_written,
                queue_depth,
            } => {
                self.bytes_read = bytes_read;
                self.bytes_written = bytes_written;
                self.queue_depth = queue_depth;
            }
            ProgressEvent::WorkerStatus {
                worker_id,
                status,
                current_chunk,
            } => {
                if let Some(worker) = self.workers.get_mut(worker_id) {
                    worker.status = status;
                    worker.current_chunk = current_chunk;
                }
            }
            ProgressEvent::WorkerChunkProgress {
                worker_id,
                seq_num,
                bytes_processed,
                bytes_written,
                total_bytes,
                finalized,
            } => {
                if let Some(worker) = self.workers.get_mut(worker_id) {
                    worker.display_chunk = Some(seq_num);
                    worker.bytes_processed = bytes_processed.min(total_bytes);
                    worker.total_bytes = total_bytes;
                    worker.bytes_written = worker.bytes_written.max(bytes_written);
                    if worker.started_at.is_none() {
                        worker.started_at = Some(Instant::now());
                    }
                    if finalized {
                        if worker.completed_at.is_none() {
                            let now = Instant::now();
                            worker.completed_at = Some(now);
                            worker.record_final_metrics(now);
                        }
                    }
                }
            }
            ProgressEvent::ChunkQueued { seq_num, .. } => {
                if !self.input_queue.contains(&seq_num) {
                    self.input_queue.push(seq_num);
                }
            }
            ProgressEvent::ChunkAssigned { worker_id, seq_num } => {
                self.input_queue.retain(|queued| *queued != seq_num);
                if let Some(worker) = self.workers.get_mut(worker_id) {
                    worker.status = "BUSY";
                    worker.current_chunk = Some(seq_num);
                    worker.display_chunk = Some(seq_num);
                    worker.bytes_processed = 0;
                    worker.total_bytes = 0;
                    worker.bytes_written = 0;
                    worker.started_at = Some(Instant::now());
                    worker.completed_at = None;
                }
            }
            ProgressEvent::ChunkPending { worker_id, seq_num } => {
                if let Some(worker) = self.workers.get_mut(worker_id) {
                    if worker.current_chunk == Some(seq_num) {
                        worker.status = "IDLE";
                        worker.current_chunk = None;
                    }
                }
                if !self.output_buffer.contains(&seq_num) {
                    self.output_buffer.push(seq_num);
                    self.output_buffer.sort_unstable();
                }
            }
            ProgressEvent::ChunkWritten { seq_num } => {
                self.output_buffer.retain(|pending| *pending != seq_num);
                self.next_expected_seq = self.next_expected_seq.max(seq_num + 1);
            }
            ProgressEvent::WorkerAvailability { worker_id, enabled } => {
                if let Some(worker) = self.workers.get_mut(worker_id) {
                    worker.status = if enabled { "IDLE" } else { "OFF" };
                    if !enabled {
                        worker.current_chunk = None;
                    }
                }
            }
            ProgressEvent::AvgCompressionTime(duration) => self.update_chunk_time(duration),
            ProgressEvent::Error(_) => self.clear_transient_pipeline_state(),
            ProgressEvent::Complete => {
                let now = Instant::now();
                let system_elapsed = now.duration_since(self.last_speed_update);
                if !system_elapsed.is_zero() {
                    self.sample_system_metrics(system_elapsed);
                    self.last_speed_update = now;
                }
                self.sample_io_bucket(now);
                self.is_complete = true;
                self.final_elapsed = Some(self.start_time.elapsed());
                self.clear_transient_pipeline_state();
            }
        }
    }

    fn clear_transient_pipeline_state(&mut self) {
        self.input_queue.clear();
        self.output_buffer.clear();
        self.queue_depth = 0;
        for worker in &mut self.workers {
            worker.current_chunk = None;
            if worker.status != "OFF" {
                worker.status = "IDLE";
            }
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
        let _ = crossterm::queue!(
            stderr,
            EnterAlternateScreen,
            crossterm::cursor::Hide,
            crossterm::event::EnableMouseCapture
        );
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
        let _ = crossterm::queue!(
            stderr,
            crossterm::cursor::Show,
            LeaveAlternateScreen,
            crossterm::event::DisableMouseCapture
        );
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
                    if let (Ok(rows), Ok(cols)) = (parts[0].parse::<u16>(), parts[1].parse::<u16>())
                    {
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
                        let cols_str = String::from_utf8_lossy(&output_cols.stdout)
                            .trim()
                            .to_string();
                        let lines_str = String::from_utf8_lossy(&output_lines.stdout)
                            .trim()
                            .to_string();
                        if let (Ok(cols), Ok(rows)) =
                            (cols_str.parse::<u16>(), lines_str.parse::<u16>())
                        {
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
    rx: std::sync::mpsc::Receiver<crate::pipeline::ProgressEvent>,
    controller: crate::pipeline::PipelineController,
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
    let hold_on_complete = std::env::var("ZIPMT_FORCE_TUI").ok().as_deref() != Some("1");
    let mut should_exit = false;
    loop {
        // Read progress events
        while let Ok(event) = rx.try_recv() {
            let mut state_guard = state.lock().unwrap();
            state_guard.apply_progress_event(event);
        }

        // Listen for terminal events with a poll tick rate
        if event::poll(tick_rate).unwrap_or(false) {
            // Drain all pending events to keep event handling responsive and avoid queue lag
            while event::poll(std::time::Duration::from_millis(0)).unwrap_or(false) {
                match event::read().unwrap() {
                    Event::Key(key) => match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                            if state.lock().unwrap().is_complete {
                                should_exit = true;
                                continue;
                            }
                            controller.abort();
                            crate::cleanup_output_file();
                            drop(guard);
                            std::process::exit(2);
                        }
                        KeyCode::Enter if state.lock().unwrap().is_complete => {
                            should_exit = true;
                        }
                        KeyCode::Char('p') | KeyCode::Char('P') => {
                            let current = controller.is_paused.load(Ordering::Relaxed);
                            if current {
                                controller.resume();
                            } else {
                                controller.pause();
                            }
                            let mut state_guard = state.lock().unwrap();
                            state_guard.is_paused = !current;
                        }
                        KeyCode::Char('i') | KeyCode::Char('I') => {
                            state.lock().unwrap().toggle_io_chart_mode();
                        }
                        KeyCode::Char('-') => {
                            let current = controller.throttle_delay_ms.load(Ordering::Relaxed);
                            let new_val = std::cmp::min(current + 50, 500);
                            controller.update_throttle(new_val);
                            let mut state_guard = state.lock().unwrap();
                            state_guard.throttle_delay_ms = new_val;
                        }
                        KeyCode::Char('+') | KeyCode::Char('=') => {
                            let current = controller.throttle_delay_ms.load(Ordering::Relaxed);
                            let new_val = current.saturating_sub(50);
                            controller.update_throttle(new_val);
                            let mut state_guard = state.lock().unwrap();
                            state_guard.throttle_delay_ms = new_val;
                        }
                        KeyCode::Char('[') => {
                            if state.lock().unwrap().mode == TuiMode::Split {
                                continue;
                            }
                            let current = controller.compression_level.load(Ordering::Relaxed);
                            if current > 1 {
                                controller.update_level(current - 1);
                                let mut state_guard = state.lock().unwrap();
                                state_guard.level = current - 1;
                            }
                        }
                        KeyCode::Char(']') => {
                            if state.lock().unwrap().mode == TuiMode::Split {
                                continue;
                            }
                            let current = controller.compression_level.load(Ordering::Relaxed);
                            if current < 9 {
                                controller.update_level(current + 1);
                                let mut state_guard = state.lock().unwrap();
                                state_guard.level = current + 1;
                            }
                        }
                        KeyCode::Tab => {
                            let mut state_guard = state.lock().unwrap();
                            state_guard.focused_widget =
                                next_focus_for_mode(state_guard.mode, state_guard.focused_widget);
                        }
                        KeyCode::PageUp => {
                            let mut state_guard = state.lock().unwrap();
                            if state_guard.mode == TuiMode::Split {
                                let page = split_page_size(state_guard.terminal_rows);
                                state_guard.split_sector_offset =
                                    state_guard.split_sector_offset.saturating_sub(page);
                            }
                        }
                        KeyCode::PageDown => {
                            let mut state_guard = state.lock().unwrap();
                            if state_guard.mode == TuiMode::Split {
                                let page = split_page_size(state_guard.terminal_rows);
                                let max_offset = state_guard.stripes.len().saturating_sub(page);
                                state_guard.split_sector_offset =
                                    (state_guard.split_sector_offset + page).min(max_offset);
                            }
                        }
                        KeyCode::Home => {
                            crate::LOG_SCROLL_OFFSET.store(usize::MAX, Ordering::Relaxed);
                        }
                        KeyCode::End => {
                            crate::LOG_SCROLL_OFFSET.store(0, Ordering::Relaxed);
                        }
                        KeyCode::Up => {
                            let mut state_guard = state.lock().unwrap();
                            match state_guard.focused_widget {
                                FocusedWidget::CompressionLevelSlider => {
                                    let current =
                                        controller.compression_level.load(Ordering::Relaxed);
                                    if current < 9 {
                                        controller.update_level(current + 1);
                                        state_guard.level = current + 1;
                                    }
                                }
                                FocusedWidget::ThrottleDelaySlider => {
                                    let current =
                                        controller.throttle_delay_ms.load(Ordering::Relaxed);
                                    let new_val = std::cmp::min(current + 50, 500);
                                    controller.update_throttle(new_val);
                                    state_guard.throttle_delay_ms = new_val;
                                }
                                FocusedWidget::ChunkSizeSlider => {
                                    let current = controller.chunk_size.load(Ordering::Relaxed);
                                    let new_val =
                                        (current * 2).min(crate::pipeline::MAX_CHUNK_SIZE);
                                    if controller.update_chunk_size(new_val) {
                                        state_guard.chunk_size = new_val;
                                    }
                                }
                                FocusedWidget::WorkerCountSlider => {
                                    let current = controller.active_workers.load(Ordering::Relaxed);
                                    let new_val = (current + 1).min(controller.max_workers);
                                    if controller.update_active_workers(new_val) {
                                        state_guard.active_workers = new_val;
                                    }
                                }
                                FocusedWidget::None => {
                                    let offset = crate::LOG_SCROLL_OFFSET.load(Ordering::Relaxed);
                                    crate::LOG_SCROLL_OFFSET
                                        .store(offset.saturating_add(1), Ordering::Relaxed);
                                }
                            }
                        }
                        KeyCode::Down => {
                            let mut state_guard = state.lock().unwrap();
                            match state_guard.focused_widget {
                                FocusedWidget::CompressionLevelSlider => {
                                    let current =
                                        controller.compression_level.load(Ordering::Relaxed);
                                    if current > 1 {
                                        controller.update_level(current - 1);
                                        state_guard.level = current - 1;
                                    }
                                }
                                FocusedWidget::ThrottleDelaySlider => {
                                    let current =
                                        controller.throttle_delay_ms.load(Ordering::Relaxed);
                                    let new_val = current.saturating_sub(50);
                                    controller.update_throttle(new_val);
                                    state_guard.throttle_delay_ms = new_val;
                                }
                                FocusedWidget::ChunkSizeSlider => {
                                    let current = controller.chunk_size.load(Ordering::Relaxed);
                                    let new_val =
                                        (current / 2).max(crate::pipeline::MIN_CHUNK_SIZE);
                                    if controller.update_chunk_size(new_val) {
                                        state_guard.chunk_size = new_val;
                                    }
                                }
                                FocusedWidget::WorkerCountSlider => {
                                    let current = controller.active_workers.load(Ordering::Relaxed);
                                    let new_val = current.saturating_sub(1).max(1);
                                    if controller.update_active_workers(new_val) {
                                        state_guard.active_workers = new_val;
                                    }
                                }
                                FocusedWidget::None => {
                                    let offset = crate::LOG_SCROLL_OFFSET.load(Ordering::Relaxed);
                                    crate::LOG_SCROLL_OFFSET
                                        .store(offset.saturating_sub(1), Ordering::Relaxed);
                                }
                            }
                        }
                        _ => {}
                    },
                    Event::Resize(w, h) => {
                        let mut state_guard = state.lock().unwrap();
                        state_guard.terminal_cols = w;
                        state_guard.terminal_rows = h;
                    }
                    Event::Mouse(mouse_event) => {
                        use crossterm::event::MouseEventKind;
                        let (w, h) = {
                            let state_guard = state.lock().unwrap();
                            (state_guard.terminal_cols, state_guard.terminal_rows)
                        };
                        let footer_height = footer_height_for_rows(h);
                        let body_height = 11 + h.saturating_sub(22) / 2;
                        let logs_top = 1 + body_height;
                        let logs_bottom = h.saturating_sub(footer_height);
                        if mouse_event.row >= logs_top && mouse_event.row < logs_bottom {
                            let offset = crate::LOG_SCROLL_OFFSET.load(Ordering::Relaxed);
                            match mouse_event.kind {
                                MouseEventKind::ScrollUp => {
                                    crate::LOG_SCROLL_OFFSET
                                        .store(offset.saturating_add(3), Ordering::Relaxed);
                                    continue;
                                }
                                MouseEventKind::ScrollDown => {
                                    crate::LOG_SCROLL_OFFSET
                                        .store(offset.saturating_sub(3), Ordering::Relaxed);
                                    continue;
                                }
                                _ => {}
                            }
                        }
                        if mouse_event.kind
                            == MouseEventKind::Down(crossterm::event::MouseButton::Left)
                            || matches!(
                                mouse_event.kind,
                                MouseEventKind::Drag(crossterm::event::MouseButton::Left)
                            )
                        {
                            if w >= 80 && h >= 22 {
                                let footer_height = footer_height_for_rows(h) as usize;
                                let controls_left = (w as usize).saturating_sub(52);
                                let footer_content_top =
                                    (h as usize).saturating_sub(footer_height) + 1;
                                let control_height = footer_height.saturating_sub(2);

                                let col = mouse_event.column as usize;
                                let row = mouse_event.row as usize;
                                if row >= footer_content_top
                                    && row < footer_content_top + control_height
                                {
                                    let mode = state.lock().unwrap().mode;
                                    if mode == TuiMode::Stream
                                        && col >= controls_left
                                        && col <= controls_left + 12
                                    {
                                        // Clicked Level Slider
                                        let fraction = slider_fraction(
                                            row - footer_content_top,
                                            control_height,
                                        );
                                        let level = (9.0 - fraction * 8.0).round() as u32;
                                        controller.update_level(level);
                                        let mut state_guard = state.lock().unwrap();
                                        state_guard.level = level;
                                        state_guard.focused_widget =
                                            FocusedWidget::CompressionLevelSlider;
                                    } else if col >= controls_left + 13 && col <= controls_left + 25
                                    {
                                        // Clicked Delay Slider
                                        let fraction = slider_fraction(
                                            row - footer_content_top,
                                            control_height,
                                        );
                                        let delay = ((1.0 - fraction) * 500.0).round() as u64;
                                        controller.update_throttle(delay);
                                        let mut state_guard = state.lock().unwrap();
                                        state_guard.throttle_delay_ms = delay;
                                        state_guard.focused_widget =
                                            FocusedWidget::ThrottleDelaySlider;
                                    } else if col >= controls_left + 26 && col <= controls_left + 38
                                    {
                                        let mut state_guard = state.lock().unwrap();
                                        let chunk_size = chunk_size_from_slider_row(
                                            row - footer_content_top,
                                            control_height,
                                        );
                                        controller.update_chunk_size(chunk_size);
                                        state_guard.chunk_size = chunk_size;
                                        state_guard.focused_widget = FocusedWidget::ChunkSizeSlider;
                                    } else if mode == TuiMode::Stream
                                        && col >= controls_left + 39
                                        && col <= controls_left + 51
                                    {
                                        let max_workers = controller.max_workers;
                                        let workers = workers_from_slider_row(
                                            row - footer_content_top,
                                            control_height,
                                            max_workers,
                                        );
                                        controller.update_active_workers(workers);
                                        let mut state_guard = state.lock().unwrap();
                                        state_guard.active_workers = workers;
                                        state_guard.focused_widget =
                                            FocusedWidget::WorkerCountSlider;
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        {
            let mut state_guard = state.lock().unwrap();

            // Append current speed/I/O data on a fixed cadence.
            let now = Instant::now();
            let sample_elapsed = now.duration_since(state_guard.last_speed_update);
            if !state_guard.is_complete && sample_elapsed >= tick_rate {
                state_guard.sample_system_metrics(sample_elapsed);
                state_guard.last_speed_update = now;
            }
            if !state_guard.is_complete {
                state_guard.sample_io_bucket(now);
            }

            let _ = draw_tui(&mut guard.terminal, &state_guard);
        }

        if should_exit || (!hold_on_complete && state.lock().unwrap().is_complete) {
            break;
        }

        if comp_handle.is_finished() {
            // Also drain any leftover events
            while let Ok(event) = rx.try_recv() {
                let mut state_guard = state.lock().unwrap();
                state_guard.apply_progress_event(event);
            }
            if !state.lock().unwrap().is_complete {
                break;
            }
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

#[allow(dead_code)]
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
        (format!("{:5.0}B ┼", max_val), "    0B ┼".to_string())
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

fn format_bytes(value: f64, per_second: bool) -> String {
    let suffix = if per_second { "/s" } else { "" };
    if value >= 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1}G{}", value / (1024.0 * 1024.0 * 1024.0), suffix)
    } else if value >= 1024.0 * 1024.0 {
        format!("{:.1}M{}", value / (1024.0 * 1024.0), suffix)
    } else if value >= 1024.0 {
        format!("{:.1}K{}", value / 1024.0, suffix)
    } else {
        format!("{:.0}B{}", value, suffix)
    }
}

fn format_rate_fixed(value: f64) -> String {
    if value >= 1024.0 * 1024.0 * 1024.0 {
        format!("{:>6.2}G/s", value / (1024.0 * 1024.0 * 1024.0))
    } else if value >= 1024.0 * 1024.0 {
        format!("{:>6.2}M/s", value / (1024.0 * 1024.0))
    } else if value >= 1024.0 {
        format!("{:>6.2}K/s", value / 1024.0)
    } else {
        format!("{value:>6.2}B/s")
    }
}

fn format_worker_ratio(ratio: Option<f64>) -> String {
    let Some(ratio) = ratio else {
        return " --.--".to_string();
    };
    if ratio > 99.99 {
        ">99.99".to_string()
    } else {
        format!("{ratio:6.2}")
    }
}

const IO_MOVING_AVERAGE_WINDOW: usize = 10;

fn moving_average(values: &[(f64, f64)], window: usize) -> Vec<(f64, f64)> {
    let window = window.max(1);
    values
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let start = (index + 1).saturating_sub(window);
            let slice = &values[start..=index];
            let (input, output) = slice
                .iter()
                .fold((0.0, 0.0), |(input_sum, output_sum), value| {
                    (input_sum + value.0, output_sum + value.1)
                });
            (input / slice.len() as f64, output / slice.len() as f64)
        })
        .collect()
}

#[derive(Debug)]
struct IoChartData {
    input: Vec<(f64, f64)>,
    output: Vec<(f64, f64)>,
    input_average: Vec<(f64, f64)>,
    output_average: Vec<(f64, f64)>,
    guides: [Vec<(f64, f64)>; 3],
    scale: f64,
    latest: (f64, f64),
    x_max: f64,
}

fn prepare_io_chart(history: &[IoSample], mode: IoChartMode, width: usize) -> IoChartData {
    let visible = history.len().min(width.saturating_mul(2).max(1));
    let samples = &history[history.len().saturating_sub(visible)..];
    let values: Vec<(f64, f64)> = samples
        .iter()
        .map(|sample| match mode {
            IoChartMode::Rate => (sample.input_rate, sample.output_rate),
            IoChartMode::Cumulative => (sample.input_total as f64, sample.output_total as f64),
        })
        .collect();
    let averages = if mode == IoChartMode::Rate {
        moving_average(&values, IO_MOVING_AVERAGE_WINDOW)
    } else {
        Vec::new()
    };
    let scale = values
        .iter()
        .flat_map(|(input, output)| [*input, *output])
        .fold(1.0_f64, f64::max);
    let latest = if mode == IoChartMode::Rate {
        averages.last().copied().unwrap_or((0.0, 0.0))
    } else {
        values.last().copied().unwrap_or((0.0, 0.0))
    };
    let x_max = values.len().saturating_sub(1).max(1) as f64;
    let input = values
        .iter()
        .enumerate()
        .map(|(index, (value, _))| (index as f64, *value))
        .collect();
    let output = values
        .iter()
        .enumerate()
        .map(|(index, (_, value))| (index as f64, -*value))
        .collect();
    let input_average = averages
        .iter()
        .enumerate()
        .map(|(index, (value, _))| (index as f64, *value))
        .collect();
    let output_average = averages
        .iter()
        .enumerate()
        .map(|(index, (_, value))| (index as f64, -*value))
        .collect();
    let guide = |level: f64| {
        (0..=x_max as usize)
            .step_by(2)
            .map(|x| (x as f64, level))
            .collect::<Vec<_>>()
    };
    IoChartData {
        input,
        output,
        input_average,
        output_average,
        guides: [guide(-scale * 0.5), guide(0.0), guide(scale * 0.5)],
        scale,
        latest,
        x_max,
    }
}

fn format_chunk_slots(chunks: &[u64], visible: usize) -> String {
    if chunks.is_empty() {
        return "--".to_string();
    }
    let mut parts: Vec<String> = chunks
        .iter()
        .take(visible)
        .map(|seq| format!("#{}", seq + 1))
        .collect();
    if chunks.len() > visible {
        parts.push(format!("+{}", chunks.len() - visible));
    }
    parts.join(" ")
}

pub fn draw_tui<B: ratatui::backend::Backend>(
    terminal: &mut ratatui::Terminal<B>,
    state: &TuiState,
) -> Result<(), std::io::Error> {
    // Standardized/mockable elapsed time for layout consistency in tests
    #[cfg(test)]
    let elapsed = state
        .final_elapsed
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(1.234);
    #[cfg(not(test))]
    let elapsed = state.elapsed().as_secs_f64();

    // Style Colors: LCARS Palette
    let style_orange = Style::default().fg(Color::Indexed(208));
    let style_purple = Style::default().fg(Color::Indexed(147));
    let style_cyan = Style::default().fg(Color::Indexed(117));
    let style_yellow = Style::default().fg(Color::Indexed(220));

    let pause_status = if state.is_paused {
        "PAUSED"
    } else if state.is_complete {
        "COMPLETE"
    } else {
        "RUNNING"
    };

    terminal.draw(|f| {
        let area = f.size();
        let cols = area.width;
        let rows = area.height;

        // Clear the screen to remove previous terminal remnants
        f.render_widget(Clear, area);

        if cols < 80 || rows < 22 {
            let warning_lines = vec![
                Line::from(Span::styled(
                    "Terminal size too small.",
                    Style::default().fg(Color::Red),
                )),
                Line::from(Span::styled(
                    format!("Current: {}x{}", cols, rows),
                    Style::default().fg(Color::Red),
                )),
                Line::from(Span::styled(
                    "Please resize to at least 80x22.",
                    Style::default().fg(Color::Yellow),
                )),
            ];
            let paragraph = Paragraph::new(warning_lines);
            f.render_widget(paragraph, area);
            return;
        }

        let rect = area;

        // Nested grid: larger terminals share extra rows across chart, logs, and knobs.
        let extra_rows = rows.saturating_sub(22);
        let body_height = 11 + extra_rows / 2;
        let footer_height = footer_height_for_rows(rows);
        let logs_height = rows.saturating_sub(1 + body_height + footer_height);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(body_height),
                Constraint::Length(logs_height),
                Constraint::Length(footer_height),
            ])
            .split(rect);

        // --- 1. Title / Header Row ---
        let status_color = match pause_status {
            "PAUSED" => Color::Red,
            "COMPLETE" => Color::Green,
            _ => Color::Indexed(117), // Cyan-blue
        };
        let title_text = "ZIPMT PIPELINE CONTROLLER ";
        let status_text = format!(" [STATUS: {}]", pause_status);
        let rule_width = (cols as usize)
            .saturating_sub(title_text.chars().count() + status_text.chars().count());
        let title_line = Line::from(vec![
            Span::styled(
                title_text,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("═".repeat(rule_width), style_orange),
            Span::styled(
                status_text,
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);
        f.render_widget(Paragraph::new(title_line), chunks[0]);

        // --- 2. Body Panels ---
        let body_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(match state.mode {
                TuiMode::Split => [Constraint::Percentage(60), Constraint::Percentage(40)],
                TuiMode::Stream => [Constraint::Percentage(58), Constraint::Percentage(42)],
            })
            .split(chunks[1]);
        let right_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(5), Constraint::Length(3)])
            .split(body_chunks[1]);

        // Left Panel Block
        let split_row_capacity = body_chunks[0].height.saturating_sub(4).max(2) as usize / 2;
        let worker_row_capacity = body_chunks[0].height.saturating_sub(4).max(4) as usize / 4;
        let split_start = state
            .split_sector_offset
            .min(state.stripes.len().saturating_sub(1));
        let split_end = (split_start + split_row_capacity).min(state.stripes.len());
        let left_title = match state.mode {
            TuiMode::Split if state.stripes.is_empty() => " Sectors -- ".to_string(),
            TuiMode::Split => format!(
                " Composite + Slices S{:02}-S{:02} of {} (+{}) ",
                split_start + 1,
                split_end,
                state.stripes.len(),
                state.stripes.len().saturating_sub(split_end)
            ),
            TuiMode::Stream if state.workers.is_empty() => " STREAM WORKERS -- ".to_string(),
            TuiMode::Stream => {
                let end = worker_row_capacity.min(state.workers.len());
                format!(
                    " STREAM WORKERS W01-W{:02} of {} (+{}) ",
                    end,
                    state.workers.len(),
                    state.workers.len().saturating_sub(end)
                )
            }
        };
        let left_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(style_orange)
            .title(Span::styled(
                left_title,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));

        // Right Panel Block
        let right_title = match state.mode {
            TuiMode::Split | TuiMode::Stream => format!(
                " I/O Flow [I] {} ",
                match state.io_chart_mode {
                    IoChartMode::Rate => "RATE MA10s",
                    IoChartMode::Cumulative => "CUMULATIVE",
                }
            ),
        };
        let right_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(style_orange)
            .title(Span::styled(
                right_title,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));

        // Render Left Panel Content
        match state.mode {
            TuiMode::Split => {
                use crate::pipeline::SplitStage;

                let mut lines = Vec::new();
                let aggregate = state.split_aggregate();
                let ratio = aggregate
                    .ratio()
                    .map(|value| format!("{value:.2}x"))
                    .unwrap_or_else(|| "--".to_string());
                let timing = if state.is_complete {
                    format!("TIME {elapsed:.1}s")
                } else {
                    let eta = composite_eta(aggregate, Duration::from_secs_f64(elapsed))
                        .map(|duration| format!("{:.0}s", duration.as_secs_f64()))
                        .unwrap_or_else(|| "--".to_string());
                    format!("ETA {eta}")
                };
                lines.push(Line::from(vec![
                    Span::styled(
                        format!(
                            "TOTAL {:3.0}% {}/{} ",
                            aggregate.percent(),
                            aggregate.completed_sectors,
                            state.stripes.len()
                        ),
                        style_yellow,
                    ),
                    Span::styled(format!("RATIO {ratio} {timing}"), style_cyan),
                ]));
                let output_io_total = aggregate
                    .output_produced
                    .saturating_add(state.split_final_bytes_written);
                let output_rate = if elapsed > 0.0 {
                    output_io_total as f64 / elapsed
                } else {
                    0.0
                };
                lines.push(Line::from(Span::styled(
                    if state.is_complete {
                        format!(
                            "IN {} OUT {} IO {} AVG {}→{}",
                            format_bytes(aggregate.input_processed as f64, false),
                            format_bytes(aggregate.output_produced as f64, false),
                            format_bytes(output_io_total as f64, false),
                            format_bytes(
                                aggregate.input_processed as f64 / elapsed.max(f64::EPSILON),
                                true
                            ),
                            format_bytes(output_rate, true)
                        )
                    } else {
                        format!(
                            "IN {} / {}  OUT {}  ACTIVE {}",
                            format_bytes(aggregate.input_processed as f64, false),
                            format_bytes(aggregate.input_total as f64, false),
                            format_bytes(aggregate.output_produced as f64, false),
                            aggregate.active_sectors
                        )
                    },
                    style_purple,
                )));
                for stripe in state
                    .stripes
                    .iter()
                    .skip(split_start)
                    .take(split_row_capacity)
                {
                    let now = Instant::now();
                    let (average_input, average_output) = stripe.average_rates(now);
                    let total = stripe.total_bytes;
                    let processed = stripe.bytes_processed.min(total);
                    let pct = if total > 0 {
                        (processed * 100) / total
                    } else {
                        0
                    };

                    let stage = match stripe.stage {
                        SplitStage::Waiting => "WAIT",
                        SplitStage::Running => "RUN ",
                        SplitStage::Done => "DONE",
                    };

                    let ratio = if stripe.stage == SplitStage::Done && stripe.bytes_written > 0 {
                        format!("{:.1}x", total as f64 / stripe.bytes_written as f64)
                    } else {
                        "--".to_string()
                    };

                    let bar_len = body_chunks[0].width.saturating_sub(30).max(3) as usize;
                    let filled = if total > 0 {
                        std::cmp::min((processed * bar_len) / total, bar_len)
                    } else {
                        0
                    };
                    let bar: String = std::iter::repeat('█')
                        .take(filled)
                        .chain(std::iter::repeat('░').take(bar_len - filled))
                        .collect();

                    lines.push(Line::from(vec![
                        Span::styled(format!("S{:02} {stage} ", stripe.id + 1), style_purple),
                        Span::styled(format!("[{}] ", bar), style_cyan),
                        Span::styled(format!("{:3}% ", pct), style_yellow),
                        Span::styled(
                            format!(
                                "{} / {}",
                                format_bytes(processed as f64, false),
                                format_bytes(total as f64, false)
                            ),
                            style_cyan,
                        ),
                    ]));
                    lines.push(Line::from(vec![
                        Span::styled("    AVG ", style_purple),
                        Span::styled(
                            format!(
                                "{}→{}  OUT {}  R {ratio}",
                                format_bytes(average_input, true),
                                format_bytes(average_output, true),
                                format_bytes(stripe.bytes_written as f64, false),
                            ),
                            style_cyan,
                        ),
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
                let speed_out = if elapsed > 0.0 {
                    state.bytes_written as f64 / elapsed
                } else {
                    0.0
                };

                f.render_widget(left_block.clone(), body_chunks[0]);
                let inner = left_block.inner(body_chunks[0]);
                let stream_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(1),
                        Constraint::Min(4),
                        Constraint::Length(1),
                    ])
                    .split(inner);

                let header_line = if state.is_complete {
                    Line::from(Span::styled(
                        format!(
                            "DONE {elapsed:.1}s I{} O{} R{ratio:.2}x",
                            format_rate_fixed(speed_in),
                            format_rate_fixed(speed_out)
                        ),
                        style_yellow,
                    ))
                } else {
                    Line::from(vec![
                        Span::styled("IN ", style_purple),
                        Span::styled(
                            format!("{:.2}M ", state.bytes_read as f64 / (1024.0 * 1024.0)),
                            style_cyan,
                        ),
                        Span::styled(format_bytes(speed_in, true), style_cyan),
                        Span::styled("  OUT ", style_purple),
                        Span::styled(
                            format!("{:.2}M ", state.bytes_written as f64 / (1024.0 * 1024.0)),
                            style_cyan,
                        ),
                        Span::styled(format_bytes(speed_out, true), style_cyan),
                        Span::styled("  R ", style_purple),
                        Span::styled(format!("{ratio:.2}x"), style_yellow),
                    ])
                };
                f.render_widget(Paragraph::new(header_line), stream_chunks[0]);

                let now = Instant::now();
                let worker_areas = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints(
                        state
                            .workers
                            .iter()
                            .take(worker_row_capacity)
                            .map(|_| Constraint::Length(4))
                            .collect::<Vec<_>>(),
                    )
                    .split(stream_chunks[1]);
                for (worker, area) in state
                    .workers
                    .iter()
                    .take(worker_row_capacity)
                    .zip(worker_areas.iter().copied())
                {
                    let progress = if worker.total_bytes > 0 {
                        worker.bytes_processed as f64 / worker.total_bytes as f64
                    } else {
                        0.0
                    };
                    let ratio = format_worker_ratio(worker.average_ratio());
                    let eta = worker
                        .eta(now)
                        .map(|duration| format!("{:.2}s", duration.as_secs_f64()))
                        .unwrap_or_else(|| "--.--s".to_string());
                    let status = if worker.completed_at.is_some() && worker.current_chunk.is_none()
                    {
                        "DONE"
                    } else {
                        worker.status
                    };
                    let chunk = worker
                        .display_chunk
                        .or(worker.current_chunk)
                        .map(|chunk| format!("C{:03}", chunk + 1))
                        .unwrap_or_else(|| "C---".to_string());
                    let card = Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(style_purple)
                        .title(Span::styled(
                            format!(" W{:02} {status:<4} {chunk} ", worker.id + 1),
                            style_yellow,
                        ));
                    let card_inner = card.inner(area);
                    f.render_widget(card, area);
                    let card_rows = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Length(1), Constraint::Length(1)])
                        .split(card_inner);
                    f.render_widget(
                        Gauge::default()
                            .gauge_style(
                                Style::default()
                                    .fg(Color::Cyan)
                                    .bg(Color::Black)
                                    .add_modifier(Modifier::BOLD),
                            )
                            .ratio(progress.clamp(0.0, 1.0))
                            .label(format!("{:6.2}%", progress * 100.0)),
                        card_rows[0],
                    );
                    f.render_widget(
                        Paragraph::new(Line::from(vec![
                            Span::styled("AVG ", style_purple),
                            Span::styled(
                                format_rate_fixed(worker.average_input_rate(now)),
                                style_cyan,
                            ),
                            Span::styled("  R ", style_purple),
                            Span::styled(format!("{ratio}x"), style_cyan),
                            Span::styled("  ETA ", style_purple),
                            Span::styled(format!("{eta:>7}"), style_cyan),
                        ])),
                        card_rows[1],
                    );
                }

                let out_queue_list = format_chunk_slots(&state.output_buffer, 8);
                let queue_list = format_chunk_slots(&state.input_queue, 4);
                f.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::styled("Q ", style_purple),
                        Span::styled(queue_list, style_cyan),
                        Span::styled(
                            format!(" ({}/{})", state.queue_depth, state.queue_capacity),
                            style_yellow,
                        ),
                        Span::styled("  PEND ", style_purple),
                        Span::styled(out_queue_list, style_cyan),
                        Span::styled(
                            format!("  N #{}", state.next_expected_seq + 1),
                            style_yellow,
                        ),
                    ])),
                    stream_chunks[2],
                );
            }
        }

        // Render Right Panel Content (native multi-series chart)
        let chart_data = prepare_io_chart(
            &state.io_history,
            state.io_chart_mode,
            right_chunks[0].width.saturating_sub(10) as usize,
        );
        let guide_style = Style::default().fg(Color::Indexed(238));
        let divider_style = Style::default().fg(Color::Indexed(240));
        let raw_input_style = Style::default().fg(Color::Cyan);
        let raw_output_style = Style::default().fg(Color::Yellow);
        let average_style = Style::default().fg(Color::Indexed(213));
        let mut datasets = vec![
            Dataset::default()
                .marker(Marker::Dot)
                .graph_type(GraphType::Scatter)
                .style(guide_style)
                .data(&chart_data.guides[0]),
            Dataset::default()
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(divider_style)
                .data(&chart_data.guides[1]),
            Dataset::default()
                .marker(Marker::Dot)
                .graph_type(GraphType::Scatter)
                .style(guide_style)
                .data(&chart_data.guides[2]),
            Dataset::default()
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(raw_input_style)
                .data(&chart_data.input),
            Dataset::default()
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(raw_output_style)
                .data(&chart_data.output),
        ];
        if state.io_chart_mode == IoChartMode::Rate {
            datasets.extend([
                Dataset::default()
                    .marker(Marker::Braille)
                    .graph_type(GraphType::Line)
                    .style(average_style)
                    .data(&chart_data.input_average),
                Dataset::default()
                    .marker(Marker::Braille)
                    .graph_type(GraphType::Line)
                    .style(average_style)
                    .data(&chart_data.output_average),
            ]);
        }
        let per_second = state.io_chart_mode == IoChartMode::Rate;
        let chart = Chart::new(datasets)
            .block(right_block)
            .x_axis(Axis::default().bounds([0.0, chart_data.x_max]))
            .y_axis(
                Axis::default()
                    .style(style_purple)
                    .bounds([-chart_data.scale, chart_data.scale])
                    .labels(vec![
                        Span::styled(
                            format!("OUT {}", format_bytes(chart_data.latest.1, per_second)),
                            style_yellow,
                        ),
                        Span::styled("0", style_purple),
                        Span::styled(
                            format!("IN {}", format_bytes(chart_data.latest.0, per_second)),
                            style_cyan,
                        ),
                    ]),
            )
            .legend_position(None);
        f.render_widget(chart, right_chunks[0]);

        let system_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(style_orange)
            .title(Span::styled(" Process ", style_yellow));
        let cpu = state
            .process_cpu_percent
            .map(|value| format!("{value:.1}%"))
            .unwrap_or_else(|| "--".to_string());
        let memory = state
            .process_memory_bytes
            .map(|value| format_bytes(value as f64, false))
            .unwrap_or_else(|| "--".to_string());
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("CPU ", style_purple),
                Span::styled(cpu, style_cyan),
                Span::styled("  RSS ", style_purple),
                Span::styled(memory, style_cyan),
            ]))
            .block(system_block),
            right_chunks[1],
        );

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
            TuiMode::Stream => format!("{:.1}MB", state.chunk_size as f64 / (1024.0 * 1024.0)),
        };
        let qcap = state.queue_capacity;
        let avg_time = state.get_avg_chunk_time_ms();
        let avg_str = if avg_time > 0.0 {
            format!("(avg:{:.1}ms)", avg_time)
        } else {
            "(avg:--)".to_string()
        };

        let logs_title = format!(
            " Log Messages (▲/▼, wheel, Home/End) ── Knobs: P:{}/{} C:{} Q:{} L:{} {} ",
            state.active_workers.min(pool_size),
            state.max_workers.max(pool_size),
            chunk_size_str.replace("MB", "M"),
            qcap,
            state.level,
            avg_str
        );
        let logs_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(style_orange)
            .title(Span::styled(
                logs_title,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));

        let logs = if let Ok(buffer) = crate::get_log_buffer().lock() {
            buffer.clone()
        } else {
            Vec::new()
        };

        let lines_back = crate::LOG_SCROLL_OFFSET.load(Ordering::Relaxed);
        let logs_inner_height = chunks[2].height.saturating_sub(2) as usize;
        let max_offset = logs.len().saturating_sub(logs_inner_height);
        let lines_back = lines_back.min(max_offset);
        let offset = log_window_start(logs.len(), logs_inner_height, lines_back);
        crate::LOG_SCROLL_OFFSET.store(lines_back, Ordering::Relaxed);

        let mut log_lines = Vec::new();
        for r in 0..logs_inner_height {
            if offset + r < logs.len() {
                let line_str = &logs[offset + r];
                let max_log_width = chunks[2].width.saturating_sub(4) as usize;
                let truncated = if line_str.len() > max_log_width {
                    format!("{}...", &line_str[..max_log_width.saturating_sub(3)])
                } else {
                    line_str.clone()
                };
                log_lines.push(Line::from(Span::styled(truncated, style_cyan)));
            } else {
                log_lines.push(Line::from(""));
            }
        }
        f.render_widget(Paragraph::new(log_lines).block(logs_block), chunks[2]);

        // --- 4. Footer Controls / Vertical Sliders ---
        let footer_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(style_orange)
            .title(Span::styled(
                " Pipeline Controls ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));

        let footer_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Min(28),
                Constraint::Length(13),
                Constraint::Length(13),
                Constraint::Length(13),
                Constraint::Length(13),
            ])
            .split(chunks[3]);

        // Left side paragraph
        let key_guides = if state.is_complete {
            vec![
                Line::from(Span::styled("[Enter/Q/Esc]Close", style_yellow)),
                Line::from(Span::styled("[I]RATE/CUMULATIVE", style_purple)),
                Line::from(Span::styled("[PgUp/PgDn]Sectors", style_purple)),
                Line::from(Span::styled("Final statistics frozen", style_purple)),
            ]
        } else if state.mode == TuiMode::Split {
            vec![
                Line::from(Span::styled("[P]Pause [-/+]Throttle", style_purple)),
                Line::from(Span::styled("[PgUp/PgDn]Sectors [Q]Exit", style_purple)),
                Line::from(Span::styled("[Tab]Throttle/Chunk [↑/↓]", style_purple)),
                Line::from(Span::styled("[I]I/O mode Level/Pool fixed", style_purple)),
            ]
        } else {
            vec![
                Line::from(Span::styled("[P]Pause [-/+]Throttle", style_purple)),
                Line::from(Span::styled("[[]/[]]Level [Q]Exit", style_purple)),
                Line::from(Span::styled("[Tab]Focus [↑/↓]Adjust", style_purple)),
                Line::from(Span::styled("[I]I/O mode Click/Drag", style_purple)),
            ]
        };
        f.render_widget(
            Paragraph::new(key_guides).block(footer_block),
            footer_chunks[0],
        );

        // Level Slider block
        let level_focused = state.focused_widget == FocusedWidget::CompressionLevelSlider;
        let level_border_style = if level_focused {
            style_yellow
        } else {
            style_purple
        };
        let level_title = if state.mode == TuiMode::Split {
            " Level FIXED "
        } else if level_focused {
            "[ Level ]"
        } else {
            " Level "
        };
        let level_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(level_border_style)
            .title(Span::styled(
                level_title,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));

        let level_val = state.level;
        let knob_height = footer_chunks[1].height.saturating_sub(2) as usize;
        let level_lines = if state.mode == TuiMode::Split {
            vec![
                "".to_string(),
                "ENCODER".to_string(),
                format!("LEVEL {level_val}"),
                "FIXED".to_string(),
            ]
        } else {
            format_knob_rows(
                knob_height,
                "9 MAX",
                &level_val.to_string(),
                "1 MIN",
                (level_val.saturating_sub(1)) as f64 / 8.0,
            )
        }
        .into_iter()
        .map(|line| Line::from(Span::styled(line, style_cyan)))
        .collect::<Vec<_>>();
        f.render_widget(
            Paragraph::new(level_lines).block(level_block),
            footer_chunks[1],
        );

        // Delay Slider block
        let delay_focused = state.focused_widget == FocusedWidget::ThrottleDelaySlider;
        let delay_border_style = if delay_focused {
            style_yellow
        } else {
            style_purple
        };
        let delay_title = if delay_focused {
            "[ Throttle ]"
        } else {
            " Throttle "
        };
        let delay_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(delay_border_style)
            .title(Span::styled(
                delay_title,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));

        let delay_val = state.throttle_delay_ms;
        let delay_lines = format_knob_rows(
            knob_height,
            "500 MAX",
            &delay_val.to_string(),
            "0 MIN",
            delay_val as f64 / 500.0,
        )
        .into_iter()
        .map(|line| Line::from(Span::styled(line, style_cyan)))
        .collect::<Vec<_>>();
        f.render_widget(
            Paragraph::new(delay_lines).block(delay_block),
            footer_chunks[2],
        );

        let chunk_focused = state.focused_widget == FocusedWidget::ChunkSizeSlider;
        let chunk_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(if chunk_focused {
                style_yellow
            } else {
                style_purple
            })
            .title(Span::styled(
                if state.mode == TuiMode::Split {
                    if chunk_focused {
                        "[ Chunk ]"
                    } else {
                        " Chunk "
                    }
                } else if chunk_focused {
                    "[ Chunk ]"
                } else {
                    " Chunk "
                },
                style_yellow,
            ));
        let chunk_kib = state.chunk_size / 1024;
        let chunk_fraction = (chunk_kib as f64 / 64.0).log2() / 7.0;
        let chunk_lines = if state.mode == TuiMode::Split {
            format_knob_rows(
                knob_height,
                "8192K",
                &format!("{}K", chunk_kib),
                "64K",
                chunk_fraction,
            )
        } else {
            format_knob_rows(
                knob_height,
                "8192K",
                &format!("{}K", chunk_kib),
                "64K",
                chunk_fraction,
            )
        }
        .into_iter()
        .map(|line| Line::from(Span::styled(line, style_cyan)))
        .collect::<Vec<_>>();
        f.render_widget(
            Paragraph::new(chunk_lines).block(chunk_block),
            footer_chunks[3],
        );

        let workers_focused = state.focused_widget == FocusedWidget::WorkerCountSlider;
        let workers_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(if workers_focused {
                style_yellow
            } else {
                style_purple
            })
            .title(Span::styled(
                if state.mode == TuiMode::Split {
                    " Pool FIXED "
                } else if workers_focused {
                    "[ Workers ]"
                } else {
                    " Workers "
                },
                style_yellow,
            ));
        let worker_fraction = if state.max_workers > 1 {
            (state.active_workers.saturating_sub(1)) as f64 / (state.max_workers - 1) as f64
        } else {
            1.0
        };
        let workers_lines = if state.mode == TuiMode::Split {
            vec![
                "".to_string(),
                "POOL".to_string(),
                format!("{}", state.max_workers),
                "FIXED".to_string(),
            ]
        } else {
            format_knob_rows(
                knob_height,
                &format!("{} MAX", state.max_workers),
                &format!("{}/{}", state.active_workers, state.max_workers),
                "1 MIN",
                worker_fraction,
            )
        }
        .into_iter()
        .map(|line| Line::from(Span::styled(line, style_cyan)))
        .collect::<Vec<_>>();
        f.render_widget(
            Paragraph::new(workers_lines).block(workers_block),
            footer_chunks[4],
        );
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_four_control_focus_order() {
        let mut focus = FocusedWidget::None;
        focus = next_focus(focus);
        assert_eq!(focus, FocusedWidget::CompressionLevelSlider);
        focus = next_focus(focus);
        assert_eq!(focus, FocusedWidget::ThrottleDelaySlider);
        focus = next_focus(focus);
        assert_eq!(focus, FocusedWidget::ChunkSizeSlider);
        focus = next_focus(focus);
        assert_eq!(focus, FocusedWidget::WorkerCountSlider);
        assert_eq!(next_focus(focus), FocusedWidget::None);
    }

    #[test]
    fn test_split_focus_visits_live_throttle_and_chunk() {
        assert_eq!(
            next_focus_for_mode(TuiMode::Split, FocusedWidget::None),
            FocusedWidget::ThrottleDelaySlider
        );
        assert_eq!(
            next_focus_for_mode(TuiMode::Split, FocusedWidget::ThrottleDelaySlider),
            FocusedWidget::ChunkSizeSlider
        );
        assert_eq!(
            next_focus_for_mode(TuiMode::Split, FocusedWidget::ChunkSizeSlider),
            FocusedWidget::None
        );
        assert_eq!(
            next_focus_for_mode(TuiMode::Split, FocusedWidget::WorkerCountSlider),
            FocusedWidget::ThrottleDelaySlider
        );
    }

    #[test]
    fn test_split_page_size_matches_responsive_body_capacity() {
        assert_eq!(split_page_size(22), 3);
        assert_eq!(split_page_size(30), 5);
    }

    #[test]
    fn test_slice_rates_freeze_and_composite_eta_uses_whole_job_average() {
        let now = Instant::now();
        let stripe = StripeProgress {
            id: 0,
            stage: crate::pipeline::SplitStage::Done,
            bytes_processed: 400,
            total_bytes: 1000,
            bytes_written: 100,
            started_at: Some(now - Duration::from_secs(4)),
            completed_at: Some(now),
        };
        assert_eq!(
            stripe.average_rates(now + Duration::from_secs(20)),
            (100.0, 25.0)
        );

        let aggregate = SplitAggregate {
            input_processed: 400,
            input_total: 1000,
            ..SplitAggregate::default()
        };
        assert_eq!(
            composite_eta(aggregate, Duration::from_secs(4)),
            Some(Duration::from_secs(6))
        );
    }

    #[test]
    fn test_process_metric_parsers() {
        let stat = "42 (zipmt worker) S 1 2 3 4 5 6 7 8 9 10 120 30 0 0";
        assert_eq!(parse_process_cpu_ticks(stat), Some(150));
        assert_eq!(
            parse_resident_memory("Name:\tzipmt\nVmRSS:\t1234 kB\n"),
            Some(1234 * 1024)
        );
    }

    #[test]
    fn test_log_window_follows_tail_and_scrolls_back() {
        assert_eq!(log_window_start(20, 5, 0), 15);
        assert_eq!(log_window_start(20, 5, 3), 12);
        assert_eq!(log_window_start(20, 5, usize::MAX), 0);
        assert_eq!(log_window_start(3, 5, 0), 0);
    }

    #[test]
    fn test_chunk_slot_overflow_is_explicit() {
        assert_eq!(format_chunk_slots(&[0, 1, 2, 3], 2), "#1 #2 +2");
        assert_eq!(format_chunk_slots(&[], 2), "--");
    }

    #[test]
    fn test_new_control_mouse_rows_map_to_valid_boundaries() {
        assert_eq!(chunk_size_from_slider_row(0, 4), 8 * 1024 * 1024);
        assert_eq!(chunk_size_from_slider_row(3, 4), 64 * 1024);
        assert_eq!(workers_from_slider_row(0, 4, 8), 8);
        assert_eq!(workers_from_slider_row(3, 4, 8), 1);
        assert_eq!(workers_from_slider_row(3, 4, 1), 1);
        assert_eq!(chunk_size_from_slider_row(7, 8), 64 * 1024);
    }

    #[test]
    fn test_stream_flow_and_four_controls_render() {
        let mut state = TuiState::new_stream(8, 0, 4, 9);
        state.input_queue = vec![0, 1, 2, 3, 4, 5, 6, 7, 8];
        state.output_buffer = vec![2, 4];
        state.workers[0].status = "BUSY";
        state.workers[0].current_chunk = Some(1);
        state.active_workers = 2;
        state.chunk_size = 256 * 1024;
        state.focused_widget = FocusedWidget::ChunkSizeSlider;

        let backend = TestBackend::new(80, 22);
        let mut terminal = Terminal::new(backend).unwrap();
        draw_tui(&mut terminal, &state).unwrap();
        let output = get_buffer_string(terminal.backend());

        assert!(output.contains("Q "));
        assert!(output.contains("[STATUS: RUNNING]"));
        assert!(output.contains("WORKERS"));
        assert!(output.contains("PEND"));
        assert!(output.contains("N #"));
        assert!(output.contains("+5"));
        assert!(output.contains("[ Chunk ]"));
        assert!(output.contains("Workers"));
        assert!(output.contains("256K"));
        assert!(output.contains("2/4"));
    }

    #[test]
    fn test_dashboard_fills_larger_terminal() {
        let state = TuiState::new_stream(8, 0, 4, 9);
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        draw_tui(&mut terminal, &state).unwrap();
        let output = get_buffer_string(terminal.backend());
        let lines: Vec<&str> = output.lines().collect();

        assert_eq!(lines.len(), 30);
        assert_eq!(lines[0].chars().count(), 120);
        assert!(lines[0].ends_with("[STATUS: RUNNING]"));
        assert!(lines[22].contains("Pipeline Controls"));
        assert!(lines[22].contains("Workers"));
        assert_eq!(footer_height_for_rows(22), 6);
        assert_eq!(footer_height_for_rows(30), 8);
    }

    #[test]
    fn test_io_sampling_keeps_rate_and_cumulative_data_across_toggle() {
        let mut state = TuiState::new_stream(8, 0, 4, 9);
        state.bytes_read = 2000;
        state.bytes_written = 1000;
        state.sample_io(Duration::from_millis(500));
        assert_eq!(state.io_history.len(), 1);
        assert_eq!(state.io_history[0].input_rate, 4000.0);
        assert_eq!(state.io_history[0].output_rate, 2000.0);
        assert_eq!(state.io_history[0].input_total, 2000);
        state.toggle_io_chart_mode();
        assert_eq!(state.io_chart_mode, IoChartMode::Cumulative);
        assert_eq!(state.io_history.len(), 1);
        state.sample_io(Duration::from_millis(100));
        assert_eq!(state.io_history[1].input_rate, 0.0);
    }

    #[test]
    fn test_graph_history_emits_consistent_one_second_buckets() {
        let mut state = TuiState::new_stream(8, 0, 2, 9);
        let start = Instant::now();
        state.last_io_sample = start;
        state.bytes_read = 1000;
        state.bytes_written = 500;
        assert!(!state.sample_io_bucket(start + Duration::from_millis(999)));
        assert!(state.io_history.is_empty());
        assert!(state.sample_io_bucket(start + Duration::from_secs(1)));
        assert_eq!(state.io_history.len(), 1);
        assert_eq!(state.io_history[0].input_rate, 1000.0);
        assert_eq!(state.io_history[0].output_rate, 500.0);
    }

    #[test]
    fn test_split_lifecycle_aggregate_and_sampling_are_truthful() {
        use crate::pipeline::{ProgressEvent, SplitStage};

        let mut state = TuiState::new_split(2, 200, 9);
        state.apply_progress_event(ProgressEvent::SplitProgress {
            stripe_id: 0,
            stage: SplitStage::Done,
            bytes_processed: 150,
            bytes_written: 50,
            total_bytes: 100,
        });
        state.apply_progress_event(ProgressEvent::SplitProgress {
            stripe_id: 1,
            stage: SplitStage::Running,
            bytes_processed: 25,
            bytes_written: 99,
            total_bytes: 100,
        });

        let aggregate = state.split_aggregate();
        assert_eq!(state.stripes[0].bytes_processed, 100);
        assert_eq!(state.stripes[1].bytes_written, 99);
        assert_eq!(aggregate.input_processed, 125);
        assert_eq!(aggregate.input_total, 200);
        assert_eq!(aggregate.output_produced, 149);
        assert_eq!(aggregate.active_sectors, 1);
        assert_eq!(aggregate.completed_sectors, 1);
        assert_eq!(aggregate.percent(), 62.5);
        assert_eq!(aggregate.ratio(), Some(2.0));
        assert_eq!(aggregate.eta(25.0), Some(Duration::from_secs(3)));

        state.apply_progress_event(ProgressEvent::SplitFinalWrite { bytes_written: 50 });
        state.sample_io(Duration::from_millis(500));
        assert_eq!(state.io_history[0].input_rate, 250.0);
        assert_eq!(state.io_history[0].output_rate, 398.0);
        assert_eq!(state.io_history[0].input_total, 125);
        assert_eq!(state.io_history[0].output_total, 199);
        state.sample_io(Duration::from_millis(100));
        assert_eq!(state.io_history[1].input_rate, 0.0);
        assert_eq!(state.io_history[1].output_rate, 0.0);
    }

    #[test]
    fn test_split_aggregate_with_no_completed_output_has_no_ratio_or_eta() {
        let state = TuiState::new_split(1, 100, 9);
        let aggregate = state.split_aggregate();
        assert_eq!(aggregate.ratio(), None);
        assert_eq!(aggregate.eta(0.0), None);
    }

    #[test]
    fn test_mirrored_io_chart_labels_and_orientation() {
        let samples = vec![IoSample {
            input_rate: 100.0,
            output_rate: 50.0,
            input_total: 100,
            output_total: 50,
        }];
        let chart = prepare_io_chart(&samples, IoChartMode::Rate, 24);
        assert_eq!(chart.input, vec![(0.0, 100.0)]);
        assert_eq!(chart.output, vec![(0.0, -50.0)]);
        assert_eq!(chart.input_average, vec![(0.0, 100.0)]);
        assert_eq!(chart.output_average, vec![(0.0, -50.0)]);
        assert_eq!(chart.latest, (100.0, 50.0));
        assert_eq!(chart.guides[1], vec![(0.0, 0.0)]);
    }

    #[test]
    fn test_rate_labels_use_ten_second_moving_average() {
        let samples = [1000.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]
            .into_iter()
            .map(|rate| IoSample {
                input_rate: rate,
                output_rate: rate / 2.0,
                input_total: 1000,
                output_total: 500,
            })
            .collect::<Vec<_>>();
        let chart = prepare_io_chart(&samples, IoChartMode::Rate, 32);
        assert_eq!(chart.latest, (100.0, 50.0));
        assert_eq!(chart.input_average.last(), Some(&(9.0, 100.0)));
        assert_eq!(chart.output_average.last(), Some(&(9.0, -50.0)));

        let cumulative = prepare_io_chart(&samples, IoChartMode::Cumulative, 32);
        assert_eq!(cumulative.latest, (1000.0, 500.0));
        assert!(cumulative.input_average.is_empty());
        assert!(cumulative.output_average.is_empty());
    }

    #[test]
    fn test_moving_average_trace_uses_distinct_color() {
        let mut state = TuiState::new_stream(8, 0, 2, 9);
        state.io_history.push(IoSample {
            input_rate: 100.0,
            output_rate: 50.0,
            input_total: 100,
            output_total: 50,
        });
        let backend = TestBackend::new(80, 22);
        let mut terminal = Terminal::new(backend).unwrap();
        draw_tui(&mut terminal, &state).unwrap();
        assert!(terminal.backend().buffer().content.iter().any(|cell| {
            cell.symbol()
                .chars()
                .next()
                .is_some_and(|ch| ('\u{2800}'..='\u{28ff}').contains(&ch))
                && cell.fg == Color::Indexed(213)
        }));
    }

    #[test]
    fn test_completion_freezes_elapsed_and_renders_final_stream_stats() {
        use crate::pipeline::ProgressEvent;

        let mut state = TuiState::new_stream(8, 1000, 2, 9);
        state.bytes_read = 1000;
        state.bytes_written = 500;
        state.apply_progress_event(ProgressEvent::Complete);
        state.final_elapsed = Some(Duration::from_secs(2));
        assert_eq!(state.elapsed(), Duration::from_secs(2));

        let backend = TestBackend::new(80, 22);
        let mut terminal = Terminal::new(backend).unwrap();
        draw_tui(&mut terminal, &state).unwrap();
        let output = get_buffer_string(terminal.backend());
        assert!(output.contains("DONE 2.0s"));
        assert!(output.contains("[Enter/Q/Esc]Close"));
        assert!(output.contains("500.00B/s"));
        assert!(output.contains("250.00B/s"));
        assert!(output.contains("2.00x"));
    }

    #[test]
    fn test_header_preserves_every_status_at_minimum_width() {
        for (paused, complete, expected) in [
            (false, false, "[STATUS: RUNNING]"),
            (true, false, "[STATUS: PAUSED]"),
            (false, true, "[STATUS: COMPLETE]"),
        ] {
            let mut state = TuiState::new_stream(8, 0, 4, 9);
            state.is_paused = paused;
            state.is_complete = complete;
            let backend = TestBackend::new(80, 22);
            let mut terminal = Terminal::new(backend).unwrap();
            draw_tui(&mut terminal, &state).unwrap();
            let output = get_buffer_string(terminal.backend());
            assert!(output.lines().next().unwrap().ends_with(expected));
        }
    }

    #[test]
    fn test_chunk_lifecycle_reducer_keeps_stages_mutually_exclusive() {
        use crate::pipeline::ProgressEvent;

        let mut state = TuiState::new_stream(8, 0, 2, 9);
        state.apply_progress_event(ProgressEvent::ChunkQueued {
            seq_num: 0,
            bytes: 1024,
        });
        assert_eq!(state.input_queue, vec![0]);

        state.apply_progress_event(ProgressEvent::ChunkAssigned {
            worker_id: 1,
            seq_num: 0,
        });
        assert!(state.input_queue.is_empty());
        assert_eq!(state.workers[1].current_chunk, Some(0));

        state.apply_progress_event(ProgressEvent::ChunkPending {
            worker_id: 1,
            seq_num: 0,
        });
        assert_eq!(state.workers[1].current_chunk, None);
        assert_eq!(state.output_buffer, vec![0]);

        state.apply_progress_event(ProgressEvent::ChunkWritten { seq_num: 0 });
        assert!(state.output_buffer.is_empty());
        assert_eq!(state.next_expected_seq, 1);
    }

    #[test]
    fn test_stream_worker_progress_renders_rate_ratio_and_eta() {
        use crate::pipeline::ProgressEvent;

        let mut state = TuiState::new_stream(8, 1000, 2, 9);
        state.apply_progress_event(ProgressEvent::ChunkAssigned {
            worker_id: 0,
            seq_num: 0,
        });
        state.apply_progress_event(ProgressEvent::WorkerChunkProgress {
            worker_id: 0,
            seq_num: 0,
            bytes_processed: 500,
            bytes_written: 250,
            total_bytes: 1000,
            finalized: false,
        });
        let started_at = Instant::now() - Duration::from_secs(2);
        state.workers[0].started_at = Some(started_at);
        state.workers[0].ratio_history.push_back(2.0);
        state.workers[0].input_rate_history.push_back(250.0);

        let backend = TestBackend::new(80, 22);
        let mut terminal = Terminal::new(backend).unwrap();
        draw_tui(&mut terminal, &state).unwrap();
        let output = get_buffer_string(terminal.backend());
        assert!(output.contains("STREAM WORKERS W01-W01"));
        assert!(output.contains("W01 BUSY C001"));
        assert!(output.contains("50.00%"));
        assert!(output.contains("R   2.00x"));
        assert!(output.contains("ETA   2.00s"));
    }

    #[test]
    fn test_stream_worker_progress_label_inverts_over_fill() {
        use crate::pipeline::ProgressEvent;

        let mut state = TuiState::new_stream(8, 1000, 1, 9);
        state.apply_progress_event(ProgressEvent::ChunkAssigned {
            worker_id: 0,
            seq_num: 0,
        });
        state.apply_progress_event(ProgressEvent::WorkerChunkProgress {
            worker_id: 0,
            seq_num: 0,
            bytes_processed: 750,
            bytes_written: 250,
            total_bytes: 1000,
            finalized: false,
        });

        let backend = TestBackend::new(80, 22);
        let mut terminal = Terminal::new(backend).unwrap();
        draw_tui(&mut terminal, &state).unwrap();
        let output = get_buffer_string(terminal.backend());
        assert!(output.contains("R  --.--x"));
        let percent_cell = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .find(|cell| cell.symbol() == "%")
            .expect("worker progress percentage should render");

        assert_eq!(percent_cell.fg, Color::Black);
        assert_eq!(percent_cell.bg, Color::Cyan);
    }

    #[test]
    fn test_worker_ratio_is_fixed_width_final_only_and_bounded() {
        assert_eq!(format_worker_ratio(None), " --.--");
        assert_eq!(format_worker_ratio(Some(2.0)), "  2.00");
        assert_eq!(format_worker_ratio(Some(1_000_000.0)), ">99.99");
    }

    #[test]
    fn test_worker_ratio_averages_completed_assignments_and_survives_reassignment() {
        use crate::pipeline::ProgressEvent;

        let mut state = TuiState::new_stream(8, 1000, 1, 9);
        for (seq_num, bytes_written) in [(0, 500), (1, 250)] {
            state.apply_progress_event(ProgressEvent::ChunkAssigned {
                worker_id: 0,
                seq_num,
            });
            state.apply_progress_event(ProgressEvent::WorkerChunkProgress {
                worker_id: 0,
                seq_num,
                bytes_processed: 1000,
                bytes_written,
                total_bytes: 1000,
                finalized: true,
            });
        }
        assert_eq!(state.workers[0].average_ratio(), Some(3.0));

        state.apply_progress_event(ProgressEvent::ChunkAssigned {
            worker_id: 0,
            seq_num: 2,
        });
        assert_eq!(state.workers[0].average_ratio(), Some(3.0));
    }

    #[test]
    fn test_worker_rate_uses_ten_chunk_window_plus_live_assignment() {
        use crate::pipeline::ProgressEvent;

        let mut state = TuiState::new_stream(8, 1000, 1, 9);
        for seq_num in 0..11 {
            state.apply_progress_event(ProgressEvent::ChunkAssigned {
                worker_id: 0,
                seq_num,
            });
            state.workers[0].started_at = Some(Instant::now() - Duration::from_secs(1));
            state.apply_progress_event(ProgressEvent::WorkerChunkProgress {
                worker_id: 0,
                seq_num,
                bytes_processed: 1000,
                bytes_written: 500,
                total_bytes: 1000,
                finalized: true,
            });
        }
        assert_eq!(state.workers[0].input_rate_history.len(), 10);

        state.apply_progress_event(ProgressEvent::ChunkAssigned {
            worker_id: 0,
            seq_num: 11,
        });
        state.workers[0].started_at = Some(Instant::now() - Duration::from_secs(2));
        state.apply_progress_event(ProgressEvent::WorkerChunkProgress {
            worker_id: 0,
            seq_num: 11,
            bytes_processed: 500,
            bytes_written: 1,
            total_bytes: 1000,
            finalized: false,
        });
        let average = state.workers[0].average_input_rate(Instant::now());
        assert!((924.0..=926.0).contains(&average));
    }

    #[test]
    fn test_chunk_lifecycle_reducer_sorts_out_of_order_pending_chunks() {
        use crate::pipeline::ProgressEvent;

        let mut state = TuiState::new_stream(8, 0, 2, 9);
        state.apply_progress_event(ProgressEvent::ChunkPending {
            worker_id: 0,
            seq_num: 2,
        });
        state.apply_progress_event(ProgressEvent::ChunkPending {
            worker_id: 1,
            seq_num: 1,
        });
        assert_eq!(state.output_buffer, vec![1, 2]);
        assert_eq!(state.next_expected_seq, 0);
    }

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
        use crate::pipeline::SplitStage;

        let mut state = TuiState::new_split(4, 400 * 1024, 6);

        state.stripes[0].stage = SplitStage::Done;
        state.stripes[0].total_bytes = 102400;
        state.stripes[0].bytes_processed = 102400;
        state.stripes[0].bytes_written = 40960;

        state.stripes[1].total_bytes = 102400;
        state.stripes[1].stage = SplitStage::Running;
        state.stripes[1].bytes_processed = 51200;
        state.stripes[1].bytes_written = 20480;

        state.stripes[2].total_bytes = 102400;
        state.stripes[2].bytes_processed = 0;
        state.stripes[2].bytes_written = 0;

        state.stripes[3].total_bytes = 102400;
        state.stripes[3].stage = SplitStage::Running;
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
    fn test_split_expanded_layout_shows_chart_paging_and_live_chunk_control() {
        use crate::pipeline::SplitStage;

        let mut state = TuiState::new_split(12, 12 * 1024, 9);
        for stripe in &mut state.stripes {
            stripe.total_bytes = 1024;
        }
        state.stripes[0].stage = SplitStage::Done;
        state.stripes[0].bytes_processed = 1024;
        state.stripes[0].bytes_written = 512;
        state.stripes[1].stage = SplitStage::Running;
        state.stripes[1].bytes_processed = 256;
        state.io_history.push(IoSample {
            input_rate: 128.0,
            output_rate: 64.0,
            input_total: 1280,
            output_total: 512,
        });
        state.process_cpu_percent = Some(37.5);
        state.process_memory_bytes = Some(24 * 1024 * 1024);

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        draw_tui(&mut terminal, &state).unwrap();
        let output = get_buffer_string(terminal.backend());

        assert!(output.contains("S01"));
        assert!(output.contains("Composite + Slices"));
        assert!(output.contains("AVG"));
        assert!(output.contains("DONE"));
        assert!(output.contains("RUN"));
        assert!(output.contains("I/O Flow [I] RATE"));
        assert!(output.contains("ENCODER"));
        assert!(output.contains("Chunk"));
        assert!(output.contains("POOL"));
        assert!(output.matches("FIXED").count() >= 2);
        assert!(output.contains("PgUp/PgDn"));
        assert!(output.contains("Process"));
        assert!(output.contains("CPU 37.5%"));
        assert!(output.contains("RSS 24.0M"));
    }

    #[test]
    fn test_tui_layout_stream_mode_snapshot() {
        let mut state = TuiState::new_stream(8, 0, 4, 6);
        state.bytes_read = 50 * 1024 * 1024;
        state.bytes_written = 20 * 1024 * 1024;
        state.queue_depth = 3;

        state.io_history = (1..=12)
            .map(|step| IoSample {
                input_rate: step as f64 * 1024.0 * 1024.0,
                output_rate: step as f64 * 512.0 * 1024.0,
                input_total: step * 1024 * 1024,
                output_total: step * 512 * 1024,
            })
            .collect();

        let backend = TestBackend::new(80, 22);
        let mut terminal = Terminal::new(backend).unwrap();
        draw_tui(&mut terminal, &state).unwrap();
        let clean_output = get_buffer_string(terminal.backend());

        insta::assert_snapshot!(clean_output);
    }

    #[test]
    fn test_stream_pipeline_chunk_labels_are_one_based() {
        let mut state = TuiState::new_stream(8, 0, 4, 9);
        state.input_queue = vec![0];
        state.workers[0].status = "BUSY";
        state.workers[0].current_chunk = Some(1);
        state.output_buffer = vec![2];
        state.next_expected_seq = 0;

        let backend = TestBackend::new(80, 22);
        let mut terminal = Terminal::new(backend).unwrap();
        draw_tui(&mut terminal, &state).unwrap();
        let output = get_buffer_string(terminal.backend());

        assert!(output.contains("#1"));
        assert!(output.contains("C002"));
        assert!(output.contains("#3"));
        assert!(output.contains("N #1"));
        assert!(!output.contains("#0"));
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
        assert!(clean_output.contains("100%"));
    }

    #[test]
    fn test_tui_layout_stream_overflow() {
        let mut state = TuiState::new_stream(8, 0, 4, 6);
        state.queue_depth = 12; // Exceeds cap (8)

        let backend = TestBackend::new(80, 22);
        let mut terminal = Terminal::new(backend).unwrap();
        draw_tui(&mut terminal, &state).unwrap();
        let clean_output = get_buffer_string(terminal.backend());
        assert!(clean_output.contains("(12/8)"));
    }

    #[test]
    fn test_query_initial_size_matches_stty() {
        let (cols, rows) = query_initial_size();

        let Ok(file) = std::fs::File::open("/dev/tty") else {
            return;
        };
        let output = std::process::Command::new("stty")
            .arg("size")
            .stdin(file)
            .output()
            .expect("Failed to run stty size");
        let out_str = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = out_str.split_whitespace().collect();
        if parts.len() == 2 {
            if let (Ok(expected_rows), Ok(expected_cols)) =
                (parts[0].parse::<u16>(), parts[1].parse::<u16>())
            {
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
            row_2_str.starts_with("╭"),
            "The full-canvas body must start below the header, got: {:?}",
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
        let mut row_2_str = String::new();
        for x in 0..120 {
            row_2_str.push_str(buffer.get(x, 1).symbol());
        }
        assert!(
            row_2_str.starts_with("╭"),
            "Expanded layouts must use the full canvas below the header, got: {:?}",
            row_2_str
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
                if line.starts_with('┌')
                    || line.starts_with('│')
                    || line.starts_with('├')
                    || line.starts_with('╰')
                {
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
                if line.starts_with('┌')
                    || line.starts_with('│')
                    || line.starts_with('├')
                    || line.starts_with('╰')
                {
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
