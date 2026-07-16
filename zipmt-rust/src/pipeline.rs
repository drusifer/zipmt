use crate::compressor::{Compressor, ZipError};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

#[derive(Debug)]
pub enum ProgressEvent {
    SplitProgress {
        stripe_id: usize,
        bytes_processed: usize,
        bytes_written: usize,
        total_bytes: usize,
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
}

impl PipelineController {
    pub fn new(level: u32) -> Self {
        Self {
            is_paused: Arc::new(AtomicBool::new(false)),
            throttle_delay_ms: Arc::new(AtomicU64::new(0)),
            compression_level: Arc::new(AtomicU32::new(level)),
            is_aborted: Arc::new(AtomicBool::new(false)),
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
}

impl CompressionPipeline {
    pub fn new(compressor: Arc<Box<dyn Compressor + Send + Sync>>, num_threads: usize) -> Self {
        Self {
            compressor,
            num_threads,
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
        let controller = PipelineController::new(6);
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
