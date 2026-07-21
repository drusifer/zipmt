use crate::compressor::{Compressor, ZipError};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};

pub const MIN_CHUNK_SIZE: usize = 64 * 1024;
pub const DEFAULT_CHUNK_SIZE: usize = 1024 * 1024;
pub const MAX_CHUNK_SIZE: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitStage {
    Waiting,
    Running,
    Done,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerStage {
    Off,
    Idle,
    Busy,
    Hold,
}

impl WorkerStage {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::Idle => "IDLE",
            Self::Busy => "BUSY",
            Self::Hold => "HOLD",
        }
    }
}

#[derive(Debug)]
pub enum ProgressEvent {
    SplitProgress {
        stripe_id: usize,
        stage: SplitStage,
        bytes_processed: usize,
        bytes_written: usize,
        total_bytes: usize,
    },
    SplitFinalWrite {
        bytes_written: usize,
    },
    StreamProgress {
        bytes_read: usize,
        bytes_written: usize,
        queue_depth: usize,
    },
    WorkerStatus {
        worker_id: usize,
        stage: WorkerStage,
        current_chunk: Option<u64>,
    },
    WorkerChunkProgress {
        worker_id: usize,
        seq_num: u64,
        bytes_processed: usize,
        bytes_written: usize,
        total_bytes: usize,
        finalized: bool,
    },
    ChunkQueued {
        seq_num: u64,
        bytes: usize,
    },
    ChunkAssigned {
        worker_id: usize,
        seq_num: u64,
    },
    ChunkPending {
        worker_id: usize,
        seq_num: u64,
    },
    ChunkWritten {
        seq_num: u64,
    },
    WorkerAvailability {
        worker_id: usize,
        enabled: bool,
    },
    AvgCompressionTime(std::time::Duration),
    Error(ZipError),
    Complete,
}

#[derive(Clone)]
pub struct ProgressSink {
    sender: std::sync::mpsc::Sender<ProgressEvent>,
}

impl ProgressSink {
    pub fn new(sender: std::sync::mpsc::Sender<ProgressEvent>) -> Self {
        Self { sender }
    }

    /// Reports progress best-effort; observer disconnect never fails the job.
    pub fn report(&self, event: ProgressEvent) -> bool {
        self.sender.send(event).is_ok()
    }
}

#[derive(Clone)]
pub struct PipelineController {
    is_paused: Arc<AtomicBool>,
    throttle_delay_ms: Arc<AtomicU64>,
    compression_level: Arc<AtomicU32>,
    is_aborted: Arc<AtomicBool>,
    chunk_size: Arc<AtomicUsize>,
    active_workers: Arc<AtomicUsize>,
    max_workers: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControllerSnapshot {
    pub is_paused: bool,
    pub throttle_delay_ms: u64,
    pub compression_level: u32,
    pub is_aborted: bool,
    pub chunk_size: usize,
    pub active_workers: usize,
    pub max_workers: usize,
}

impl PipelineController {
    pub fn new(level: u32) -> Self {
        let max_workers = std::thread::available_parallelism()
            .map(|count| count.get())
            .unwrap_or(4);
        Self::new_with_workers(level, max_workers)
    }

    pub fn new_with_workers(level: u32, max_workers: usize) -> Self {
        let max_workers = max_workers.max(1);
        Self {
            is_paused: Arc::new(AtomicBool::new(false)),
            throttle_delay_ms: Arc::new(AtomicU64::new(0)),
            compression_level: Arc::new(AtomicU32::new(level)),
            is_aborted: Arc::new(AtomicBool::new(false)),
            chunk_size: Arc::new(AtomicUsize::new(DEFAULT_CHUNK_SIZE)),
            active_workers: Arc::new(AtomicUsize::new(max_workers)),
            max_workers,
        }
    }

    pub fn update_level(&self, level: u32) {
        if (1..=9).contains(&level) {
            self.compression_level.store(level, Ordering::Relaxed);
        }
    }

    pub fn update_throttle(&self, delay_ms: u64) {
        if delay_ms <= 500 {
            self.throttle_delay_ms.store(delay_ms, Ordering::Relaxed);
        }
    }

    pub fn update_chunk_size(&self, bytes: usize) -> bool {
        let valid = (MIN_CHUNK_SIZE..=MAX_CHUNK_SIZE).contains(&bytes) && bytes.is_power_of_two();
        if valid {
            self.chunk_size.store(bytes, Ordering::Release);
        }
        valid
    }

    pub fn update_active_workers(&self, count: usize) -> bool {
        if (1..=self.max_workers).contains(&count) {
            self.active_workers.store(count, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    pub fn pause(&self) {
        self.is_paused.store(true, Ordering::Relaxed);
    }

    pub fn resume(&self) {
        self.is_paused.store(false, Ordering::Relaxed);
    }

    pub fn abort(&self) {
        self.is_aborted.store(true, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> ControllerSnapshot {
        ControllerSnapshot {
            is_paused: self.is_paused.load(Ordering::Relaxed),
            throttle_delay_ms: self.throttle_delay_ms.load(Ordering::Relaxed),
            compression_level: self.compression_level.load(Ordering::Relaxed),
            is_aborted: self.is_aborted.load(Ordering::Relaxed),
            chunk_size: self.chunk_size.load(Ordering::Acquire),
            active_workers: self.active_workers.load(Ordering::Relaxed),
            max_workers: self.max_workers,
        }
    }

    pub fn is_aborted(&self) -> bool {
        self.is_aborted.load(Ordering::Relaxed)
    }

    pub fn is_paused(&self) -> bool {
        self.is_paused.load(Ordering::Relaxed)
    }

    pub fn compression_level(&self) -> u32 {
        self.compression_level.load(Ordering::Relaxed)
    }

    pub fn throttle_delay_ms(&self) -> u64 {
        self.throttle_delay_ms.load(Ordering::Relaxed)
    }

    pub fn chunk_size(&self) -> usize {
        self.chunk_size.load(Ordering::Acquire)
    }

    pub fn active_workers(&self) -> usize {
        self.active_workers.load(Ordering::Relaxed)
    }

    pub fn max_workers(&self) -> usize {
        self.max_workers
    }
}

pub enum InputSource {
    File(PathBuf),
    Stdin,
}

pub enum OutputDestination {
    File(PathBuf),
    Stdout,
}

fn run_file_to_file(
    input: PathBuf,
    output: PathBuf,
    compressor: &dyn Compressor,
    threads: usize,
    progress: std::sync::mpsc::Sender<ProgressEvent>,
    controller: &PipelineController,
) -> Result<(), ZipError> {
    crate::split_mode::compress_file(&input, &output, compressor, threads, progress, controller)
}

fn run_file_to_stdout(
    input: PathBuf,
    compressor: &dyn Compressor,
    threads: usize,
    progress: std::sync::mpsc::Sender<ProgressEvent>,
    controller: &PipelineController,
) -> Result<(), ZipError> {
    let mut input = std::fs::File::open(input)?;
    let mut output = std::io::stdout();
    crate::stream_mode::compress_stream(
        &mut input,
        &mut output,
        compressor,
        threads,
        progress,
        controller,
    )
}

fn run_stdin_to_file(
    output: PathBuf,
    compressor: &dyn Compressor,
    threads: usize,
    progress: std::sync::mpsc::Sender<ProgressEvent>,
    controller: &PipelineController,
) -> Result<(), ZipError> {
    let mut input = std::io::stdin();
    let mut output = std::fs::File::create(output)?;
    crate::stream_mode::compress_stream(
        &mut input,
        &mut output,
        compressor,
        threads,
        progress,
        controller,
    )
}

fn run_stdin_to_stdout(
    compressor: &dyn Compressor,
    threads: usize,
    progress: std::sync::mpsc::Sender<ProgressEvent>,
    controller: &PipelineController,
) -> Result<(), ZipError> {
    let mut input = std::io::stdin();
    let mut output = std::io::stdout();
    crate::stream_mode::compress_stream(
        &mut input,
        &mut output,
        compressor,
        threads,
        progress,
        controller,
    )
}

pub struct CompressionPipeline {
    compressor: Arc<dyn Compressor + Send + Sync>,
    num_threads: usize,
    initial_level: u32,
}

impl CompressionPipeline {
    pub fn new(compressor: Arc<dyn Compressor + Send + Sync>, num_threads: usize) -> Self {
        Self::with_level(compressor, num_threads, crate::DEFAULT_COMPRESSION_LEVEL)
    }

    pub fn with_level(
        compressor: Arc<dyn Compressor + Send + Sync>,
        num_threads: usize,
        initial_level: u32,
    ) -> Self {
        Self {
            compressor,
            num_threads,
            initial_level,
        }
    }

    pub fn run(
        &self,
        input_source: InputSource,
        output_dest: OutputDestination,
    ) -> (
        PipelineController,
        std::sync::mpsc::Receiver<ProgressEvent>,
        std::thread::JoinHandle<Result<(), ZipError>>,
    ) {
        let (tx, rx) = std::sync::mpsc::channel();
        let progress = ProgressSink::new(tx.clone());
        let resolved_workers = if self.num_threads > 0 {
            self.num_threads
        } else {
            std::thread::available_parallelism()
                .map(|count| count.get())
                .unwrap_or(4)
        };
        let controller = PipelineController::new_with_workers(self.initial_level, resolved_workers);
        let controller_clone = controller.clone();
        let compressor = self.compressor.clone();
        let num_threads = self.num_threads;

        let handle = std::thread::spawn(move || {
            let res = match (input_source, output_dest) {
                (InputSource::File(in_path), OutputDestination::File(out_path)) => {
                    run_file_to_file(
                        in_path,
                        out_path,
                        compressor.as_ref(),
                        num_threads,
                        tx.clone(),
                        &controller_clone,
                    )
                }
                (InputSource::File(in_path), OutputDestination::Stdout) => run_file_to_stdout(
                    in_path,
                    compressor.as_ref(),
                    num_threads,
                    tx.clone(),
                    &controller_clone,
                ),
                (InputSource::Stdin, OutputDestination::File(out_path)) => run_stdin_to_file(
                    out_path,
                    compressor.as_ref(),
                    num_threads,
                    tx.clone(),
                    &controller_clone,
                ),
                (InputSource::Stdin, OutputDestination::Stdout) => run_stdin_to_stdout(
                    compressor.as_ref(),
                    num_threads,
                    tx.clone(),
                    &controller_clone,
                ),
            };

            match res {
                Ok(_) => {
                    progress.report(ProgressEvent::Complete);
                    Ok(())
                }
                Err(e) => {
                    progress.report(ProgressEvent::Error(e.clone()));
                    Err(e)
                }
            }
        });

        (controller, rx, handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_snapshot_exposes_consistent_validated_state() {
        let controller = PipelineController::new_with_workers(6, 4);
        controller.pause();
        controller.update_throttle(150);
        assert!(controller.update_chunk_size(128 * 1024));
        assert!(controller.update_active_workers(2));
        let snapshot = controller.snapshot();

        assert!(snapshot.is_paused);
        assert_eq!(snapshot.throttle_delay_ms, 150);
        assert_eq!(snapshot.compression_level, 6);
        assert_eq!(snapshot.chunk_size, 128 * 1024);
        assert_eq!(snapshot.active_workers, 2);
        assert_eq!(snapshot.max_workers, 4);
    }

    #[test]
    fn disconnected_progress_observer_is_explicitly_best_effort() {
        let (sender, receiver) = std::sync::mpsc::channel();
        let progress = ProgressSink::new(sender);
        drop(receiver);

        assert!(!progress.report(ProgressEvent::Complete));
    }

    #[test]
    fn controller_validates_chunk_size_boundaries() {
        let controller = PipelineController::new_with_workers(9, 4);
        assert_eq!(
            controller.chunk_size.load(Ordering::Relaxed),
            DEFAULT_CHUNK_SIZE
        );
        assert!(controller.update_chunk_size(MIN_CHUNK_SIZE));
        assert!(controller.update_chunk_size(MAX_CHUNK_SIZE));
        assert!(!controller.update_chunk_size(MIN_CHUNK_SIZE - 1));
        assert!(!controller.update_chunk_size(3 * MIN_CHUNK_SIZE));
        assert_eq!(
            controller.chunk_size.load(Ordering::Relaxed),
            MAX_CHUNK_SIZE
        );
    }

    #[test]
    fn controller_validates_active_worker_boundaries() {
        let controller = PipelineController::new_with_workers(9, 4);
        assert_eq!(controller.active_workers.load(Ordering::Relaxed), 4);
        assert!(controller.update_active_workers(1));
        assert!(controller.update_active_workers(4));
        assert!(!controller.update_active_workers(0));
        assert!(!controller.update_active_workers(5));
        assert_eq!(controller.active_workers.load(Ordering::Relaxed), 4);
    }
}
