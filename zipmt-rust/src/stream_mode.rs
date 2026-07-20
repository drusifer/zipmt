use crate::compressor::{Compressor, ZipError};
use crate::pipeline::{PipelineController, ProgressEvent, ProgressSink, WorkerStage};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, SyncSender, channel, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread;

pub struct Block {
    pub seq_num: u64,
    pub data: Vec<u8>,
}

pub struct CompressedBlock {
    pub worker_id: usize,
    pub seq_num: u64,
    pub data: Result<Vec<u8>, ZipError>,
}

#[derive(Clone)]
struct StreamMetrics {
    bytes_read: Arc<AtomicUsize>,
    bytes_written: Arc<AtomicUsize>,
    queue_depth: Arc<AtomicUsize>,
}

struct WorkerRuntime<'a> {
    jobs: Arc<Mutex<Receiver<Block>>>,
    retry: SyncSender<Block>,
    results: Sender<CompressedBlock>,
    progress: ProgressSink,
    controller: PipelineController,
    metrics: StreamMetrics,
    reader_done: Arc<AtomicBool>,
    compressor: &'a (dyn Compressor + Send + Sync),
}

fn receive_job(runtime: &WorkerRuntime<'_>) -> Option<Option<Block>> {
    let receiver = runtime.jobs.lock().unwrap();
    match receiver.recv_timeout(std::time::Duration::from_millis(25)) {
        Ok(block) => Some(Some(block)),
        Err(RecvTimeoutError::Timeout) => {
            let finished = runtime.reader_done.load(Ordering::Relaxed)
                && runtime.metrics.queue_depth.load(Ordering::Relaxed) == 0;
            (!finished).then_some(None)
        }
        Err(RecvTimeoutError::Disconnected) => None,
    }
}

fn compress_job(worker_id: usize, block: Block, runtime: &WorkerRuntime<'_>) -> bool {
    if worker_id >= runtime.controller.active_workers() {
        return runtime.retry.send(block).is_ok();
    }
    let queue_depth = runtime.metrics.queue_depth.fetch_sub(1, Ordering::Relaxed) - 1;
    runtime.progress.report(ProgressEvent::StreamProgress {
        bytes_read: runtime.metrics.bytes_read.load(Ordering::Relaxed),
        bytes_written: runtime.metrics.bytes_written.load(Ordering::Relaxed),
        queue_depth,
    });
    runtime.progress.report(ProgressEvent::WorkerStatus {
        worker_id,
        stage: WorkerStage::Busy,
        current_chunk: Some(block.seq_num),
    });
    runtime.progress.report(ProgressEvent::ChunkAssigned {
        worker_id,
        seq_num: block.seq_num,
    });

    let progress = runtime.progress.clone();
    let total_bytes = block.data.len();
    let seq_num = block.seq_num;
    let processed = Arc::new(AtomicUsize::new(0));
    let processed_callback = Arc::clone(&processed);
    let compressed = runtime.compressor.compress_with_progress(
        &block.data,
        &move |input_bytes, output_bytes, duration| {
            let current =
                processed_callback.fetch_add(input_bytes, Ordering::Relaxed) + input_bytes;
            progress.report(ProgressEvent::WorkerChunkProgress {
                worker_id,
                seq_num,
                bytes_processed: current.min(total_bytes),
                bytes_written: output_bytes,
                total_bytes,
                finalized: false,
            });
            progress.report(ProgressEvent::AvgCompressionTime(duration));
        },
        &runtime.controller,
    );
    let compressed_len = compressed.as_ref().map_or(0, Vec::len);
    runtime.progress.report(ProgressEvent::WorkerChunkProgress {
        worker_id,
        seq_num,
        bytes_processed: total_bytes,
        bytes_written: compressed_len,
        total_bytes,
        finalized: true,
    });
    runtime.progress.report(ProgressEvent::WorkerStatus {
        worker_id,
        stage: WorkerStage::Hold,
        current_chunk: Some(seq_num),
    });
    if runtime
        .results
        .send(CompressedBlock {
            worker_id,
            seq_num,
            data: compressed,
        })
        .is_err()
    {
        return false;
    }
    runtime.progress.report(ProgressEvent::WorkerStatus {
        worker_id,
        stage: WorkerStage::Idle,
        current_chunk: None,
    });
    true
}

fn run_worker(worker_id: usize, runtime: WorkerRuntime<'_>) {
    crate::log_verbose!("Worker thread {} started", worker_id);
    let mut last_enabled = None;
    loop {
        if runtime.controller.is_aborted() {
            break;
        }
        let enabled = worker_id < runtime.controller.active_workers();
        if last_enabled != Some(enabled) {
            runtime
                .progress
                .report(ProgressEvent::WorkerAvailability { worker_id, enabled });
            last_enabled = Some(enabled);
        }
        if !enabled {
            if runtime.reader_done.load(Ordering::Relaxed)
                && runtime.metrics.queue_depth.load(Ordering::Relaxed) == 0
            {
                break;
            }
            thread::sleep(std::time::Duration::from_millis(25));
            continue;
        }
        match receive_job(&runtime) {
            Some(Some(block)) => {
                if !compress_job(worker_id, block, &runtime) {
                    break;
                }
            }
            Some(None) => {}
            None => break,
        }
    }
    crate::log_verbose!("Worker thread {} exiting", worker_id);
}

fn read_blocks(
    input: &mut (dyn Read + Send),
    jobs: SyncSender<Block>,
    progress: ProgressSink,
    controller: PipelineController,
    metrics: StreamMetrics,
    reader_done: Arc<AtomicBool>,
) -> Result<(), ZipError> {
    let result = read_blocks_inner(input, &jobs, &progress, &controller, &metrics);
    reader_done.store(true, Ordering::Relaxed);
    result
}

fn read_blocks_inner(
    input: &mut (dyn Read + Send),
    jobs: &SyncSender<Block>,
    progress: &ProgressSink,
    controller: &PipelineController,
    metrics: &StreamMetrics,
) -> Result<(), ZipError> {
    let mut seq_num = 0u64;
    loop {
        if controller.is_aborted() {
            break;
        }
        while controller.is_paused() {
            if controller.is_aborted() {
                return Ok(());
            }
            thread::sleep(std::time::Duration::from_millis(50));
        }
        let chunk_size = controller.chunk_size();
        let mut buffer = vec![0u8; chunk_size];
        let mut size = 0;
        while size < chunk_size && !controller.is_aborted() {
            let bytes = input.read(&mut buffer[size..]).map_err(ZipError::Io)?;
            if bytes == 0 {
                break;
            }
            size += bytes;
        }
        if size == 0 {
            break;
        }
        let total = metrics.bytes_read.fetch_add(size, Ordering::Relaxed) + size;
        let queue_depth = metrics.queue_depth.fetch_add(1, Ordering::Relaxed) + 1;
        progress.report(ProgressEvent::StreamProgress {
            bytes_read: total,
            bytes_written: metrics.bytes_written.load(Ordering::Relaxed),
            queue_depth,
        });
        buffer.truncate(size);
        progress.report(ProgressEvent::ChunkQueued {
            seq_num,
            bytes: size,
        });
        if jobs
            .send(Block {
                seq_num,
                data: buffer,
            })
            .is_err()
        {
            break;
        }
        seq_num += 1;
    }
    Ok(())
}

struct OrderedWriter<'a> {
    output: &'a mut dyn Write,
    progress: ProgressSink,
    controller: PipelineController,
    metrics: StreamMetrics,
    pending: BTreeMap<u64, Vec<u8>>,
    next_seq_num: u64,
}

impl OrderedWriter<'_> {
    fn flush_ready(&mut self) -> Result<(), ZipError> {
        while let Some(data) = self.pending.remove(&self.next_seq_num) {
            let total = self
                .metrics
                .bytes_written
                .fetch_add(data.len(), Ordering::Relaxed)
                + data.len();
            self.progress.report(ProgressEvent::StreamProgress {
                bytes_read: self.metrics.bytes_read.load(Ordering::Relaxed),
                bytes_written: total,
                queue_depth: self.metrics.queue_depth.load(Ordering::Relaxed),
            });
            self.output.write_all(&data)?;
            self.progress.report(ProgressEvent::ChunkWritten {
                seq_num: self.next_seq_num,
            });
            self.next_seq_num += 1;
        }
        Ok(())
    }

    fn run(&mut self, results: Receiver<CompressedBlock>) -> Result<(), ZipError> {
        for result in results {
            if self.controller.is_aborted() {
                break;
            }
            let compressed = result.data?;
            self.pending.insert(result.seq_num, compressed);
            self.progress.report(ProgressEvent::ChunkPending {
                worker_id: result.worker_id,
                seq_num: result.seq_num,
            });
            self.flush_ready()?;
        }
        self.flush_ready()
    }
}

/// Compresses data from `input` reader to `output` writer in Stream Mode using channels.
pub fn compress_stream(
    input: &mut (dyn Read + Send),
    output: &mut dyn Write,
    compressor: &(dyn Compressor + Send + Sync),
    num_threads: usize,
    sender: Sender<ProgressEvent>,
    controller: &PipelineController,
) -> Result<(), ZipError> {
    let progress = ProgressSink::new(sender);
    let pool_size = if num_threads > 0 {
        num_threads
    } else {
        num_threads_default()
    };
    crate::log_verbose!(
        "Starting pipeline stream compression with pool size: {}",
        pool_size
    );

    // Setup channels with backpressure bounds
    let (job_tx, job_rx) = sync_channel::<Block>(pool_size * 2);
    let (result_tx, result_rx) = channel::<CompressedBlock>();

    let job_rx = Arc::new(Mutex::new(job_rx));

    let bytes_read = Arc::new(AtomicUsize::new(0));
    let bytes_written = Arc::new(AtomicUsize::new(0));
    let queue_depth = Arc::new(AtomicUsize::new(0));
    let reader_done = Arc::new(AtomicBool::new(false));
    let metrics = StreamMetrics {
        bytes_read: Arc::clone(&bytes_read),
        bytes_written: Arc::clone(&bytes_written),
        queue_depth: Arc::clone(&queue_depth),
    };

    // Use scoped threads to safely pass non-static references across thread boundaries
    let scope_res = thread::scope(|s| {
        let mut workers = Vec::new();

        // Spawn worker threads
        for worker_id in 0..pool_size {
            let runtime = WorkerRuntime {
                jobs: Arc::clone(&job_rx),
                retry: job_tx.clone(),
                results: result_tx.clone(),
                progress: progress.clone(),
                controller: controller.clone(),
                metrics: metrics.clone(),
                reader_done: Arc::clone(&reader_done),
                compressor,
            };
            let handle = s.spawn(move || run_worker(worker_id, runtime));
            workers.push(handle);
        }

        // Drop the local result_tx so that result_rx closes when all workers exit
        drop(result_tx);

        // Spawn reader thread
        let reader_progress = progress.clone();
        let reader_controller = controller.clone();
        let reader_metrics = metrics.clone();
        let reader_finished = Arc::clone(&reader_done);
        let reader_handle = s.spawn(move || {
            read_blocks(
                input,
                job_tx,
                reader_progress,
                reader_controller,
                reader_metrics,
                reader_finished,
            )
        });

        let writer_result = OrderedWriter {
            output,
            progress: progress.clone(),
            controller: controller.clone(),
            metrics: metrics.clone(),
            pending: BTreeMap::new(),
            next_seq_num: 0,
        }
        .run(result_rx);

        // Join reader thread and check for errors
        let reader_join = reader_handle.join();
        if let Err(panic_payload) = reader_join {
            return Err(ZipError::Compression(format!(
                "Reader thread panicked: {:?}",
                panic_payload
            )));
        }
        reader_join.unwrap()?;

        writer_result
    });

    scope_res?;
    output.flush()?;
    crate::log_verbose!("Stream compression completed successfully.");
    Ok(())
}

fn num_threads_default() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compressor::GzipCompressor;
    use std::io::Cursor;

    struct RuntimeControlReader {
        inner: Cursor<Vec<u8>>,
        controller: PipelineController,
        changed: bool,
    }

    impl Read for RuntimeControlReader {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let read = self.inner.read(buffer)?;
            if read > 0 && !self.changed {
                assert!(self.controller.update_chunk_size(64 * 1024));
                assert!(self.controller.update_active_workers(1));
                self.changed = true;
            }
            Ok(read)
        }
    }

    #[test]
    fn test_stream_mode_compression() {
        let original_data =
            b"This is some pipeline stream data that we want to compress. ".repeat(200000); // ~12MB
        let mut input_cursor = Cursor::new(original_data.clone());
        let mut output_buffer = Vec::new();

        let compressor = GzipCompressor { level: 6 };
        let (tx, rx) = std::sync::mpsc::channel();
        let controller = PipelineController::new(6);
        let result = compress_stream(
            &mut input_cursor,
            &mut output_buffer,
            &compressor,
            4,
            tx,
            &controller,
        );
        assert!(
            result.is_ok(),
            "Stream mode compression failed: {:?}",
            result.err()
        );

        // Decompress and verify
        let mut decoder = flate2::read::MultiGzDecoder::new(&output_buffer[..]);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed).unwrap();

        assert_eq!(
            original_data.to_vec(),
            decompressed,
            "Decompressed stream mismatch"
        );

        let events: Vec<_> = rx.try_iter().collect();
        assert!(
            events
                .iter()
                .any(|event| matches!(event, ProgressEvent::ChunkQueued { seq_num: 0, .. }))
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, ProgressEvent::ChunkAssigned { seq_num: 0, .. }))
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, ProgressEvent::ChunkPending { seq_num: 0, .. }))
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, ProgressEvent::ChunkWritten { seq_num: 0 }))
        );
    }

    #[test]
    fn test_runtime_controls_preserve_stream_bytes_and_order() {
        let original_data = b"runtime-controls-ordering".repeat(60_000);
        let controller = PipelineController::new_with_workers(9, 4);
        let mut input = RuntimeControlReader {
            inner: Cursor::new(original_data.clone()),
            controller: controller.clone(),
            changed: false,
        };
        let mut output = Vec::new();
        let compressor = GzipCompressor { level: 9 };
        let (tx, rx) = std::sync::mpsc::channel();

        compress_stream(&mut input, &mut output, &compressor, 4, tx, &controller).unwrap();

        let mut decoder = flate2::read::MultiGzDecoder::new(&output[..]);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed).unwrap();
        assert_eq!(decompressed, original_data);

        let events: Vec<_> = rx.try_iter().collect();
        let queued_sizes: Vec<usize> = events
            .iter()
            .filter_map(|event| match event {
                ProgressEvent::ChunkQueued { bytes, .. } => Some(*bytes),
                _ => None,
            })
            .collect();
        assert_eq!(queued_sizes.first(), Some(&(1024 * 1024)));
        assert!(queued_sizes.iter().skip(1).any(|bytes| *bytes == 64 * 1024));

        let written: Vec<u64> = events
            .iter()
            .filter_map(|event| match event {
                ProgressEvent::ChunkWritten { seq_num } => Some(*seq_num),
                _ => None,
            })
            .collect();
        assert_eq!(written, (0..written.len() as u64).collect::<Vec<_>>());
        assert!(events.iter().any(|event| matches!(
            event,
            ProgressEvent::WorkerAvailability {
                worker_id: 1..,
                enabled: false
            }
        )));
    }
}
