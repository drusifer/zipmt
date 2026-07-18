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
        status: &'static str,
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
pub struct PipelineController {
    pub is_paused: Arc<AtomicBool>,
    pub throttle_delay_ms: Arc<AtomicU64>,
    pub compression_level: Arc<AtomicU32>,
    pub is_aborted: Arc<AtomicBool>,
    pub chunk_size: Arc<AtomicUsize>,
    pub active_workers: Arc<AtomicUsize>,
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
        if level >= 1 && level <= 9 {
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
}

pub enum InputSource {
    File(PathBuf),
    Stdin,
}

pub enum OutputDestination {
    File(PathBuf),
    Stdout,
}

pub struct CompressionPipeline {
    compressor: Arc<Box<dyn Compressor + Send + Sync>>,
    num_threads: usize,
    initial_level: u32,
}

impl CompressionPipeline {
    pub fn new(compressor: Arc<Box<dyn Compressor + Send + Sync>>, num_threads: usize) -> Self {
        Self::with_level(compressor, num_threads, crate::DEFAULT_COMPRESSION_LEVEL)
    }

    pub fn with_level(
        compressor: Arc<Box<dyn Compressor + Send + Sync>>,
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
                    crate::split_mode::compress_file(
                        &in_path,
                        &out_path,
                        compressor.as_ref().as_ref(),
                        num_threads,
                        tx.clone(),
                        &controller_clone,
                    )
                }
                (InputSource::File(in_path), OutputDestination::Stdout) => {
                    match std::fs::read(&in_path) {
                        Ok(data) => {
                            let mut cursor = std::io::Cursor::new(data);
                            let mut stdout = std::io::stdout();
                            crate::stream_mode::compress_stream(
                                &mut cursor,
                                &mut stdout,
                                compressor.as_ref().as_ref(),
                                num_threads,
                                tx.clone(),
                                &controller_clone,
                            )
                        }
                        Err(e) => Err(ZipError::Io(e)),
                    }
                }
                (InputSource::Stdin, OutputDestination::File(out_path)) => {
                    let mut stdin = std::io::stdin();
                    match std::fs::File::create(&out_path) {
                        Ok(mut out_file) => crate::stream_mode::compress_stream(
                            &mut stdin,
                            &mut out_file,
                            compressor.as_ref().as_ref(),
                            num_threads,
                            tx.clone(),
                            &controller_clone,
                        ),
                        Err(e) => Err(ZipError::Io(e)),
                    }
                }
                (InputSource::Stdin, OutputDestination::Stdout) => {
                    let mut stdin = std::io::stdin();
                    let mut stdout = std::io::stdout();
                    crate::stream_mode::compress_stream(
                        &mut stdin,
                        &mut stdout,
                        compressor.as_ref().as_ref(),
                        num_threads,
                        tx.clone(),
                        &controller_clone,
                    )
                }
            };

            match res {
                Ok(_) => {
                    let _ = tx.send(ProgressEvent::Complete);
                    Ok(())
                }
                Err(e) => {
                    let _ = tx.send(ProgressEvent::Error(e.clone()));
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
