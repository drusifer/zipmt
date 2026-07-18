use crate::compressor::{Compressor, ZipError};
use crate::pipeline::{PipelineController, ProgressEvent};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{RecvTimeoutError, Sender, channel, sync_channel};
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

/// Compresses data from `input` reader to `output` writer in Stream Mode using channels.
pub fn compress_stream(
    input: &mut (dyn Read + Send),
    output: &mut dyn Write,
    compressor: &(dyn Compressor + Send + Sync),
    num_threads: usize,
    sender: Sender<ProgressEvent>,
    controller: &PipelineController,
) -> Result<(), ZipError> {
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

    // Use scoped threads to safely pass non-static references across thread boundaries
    let scope_res = thread::scope(|s| {
        let mut workers = Vec::new();

        // Spawn worker threads
        for worker_id in 0..pool_size {
            let job_rx = Arc::clone(&job_rx);
            let job_tx_worker = job_tx.clone();
            let result_tx = result_tx.clone();
            let tx = sender.clone();
            let ctrl = controller.clone();
            let bytes_read_clone = bytes_read.clone();
            let bytes_written_clone = bytes_written.clone();
            let queue_depth_clone = queue_depth.clone();
            let reader_done_clone = reader_done.clone();

            let handle = s.spawn(move || {
                crate::log_verbose!("Worker thread {} started", worker_id);
                let mut last_enabled = None;
                loop {
                    if ctrl.is_aborted.load(Ordering::Relaxed) {
                        break;
                    }
                    let enabled = worker_id < ctrl.active_workers.load(Ordering::Relaxed);
                    if last_enabled != Some(enabled) {
                        let _ = tx.send(ProgressEvent::WorkerAvailability { worker_id, enabled });
                        last_enabled = Some(enabled);
                    }
                    if !enabled {
                        if reader_done_clone.load(Ordering::Relaxed)
                            && queue_depth_clone.load(Ordering::Relaxed) == 0
                        {
                            break;
                        }
                        thread::sleep(std::time::Duration::from_millis(25));
                        continue;
                    }
                    let block_opt = {
                        let rx_guard = job_rx.lock().unwrap();
                        match rx_guard.recv_timeout(std::time::Duration::from_millis(25)) {
                            Ok(block) => Some(block),
                            Err(RecvTimeoutError::Timeout) => {
                                if reader_done_clone.load(Ordering::Relaxed)
                                    && queue_depth_clone.load(Ordering::Relaxed) == 0
                                {
                                    break;
                                }
                                continue;
                            }
                            Err(RecvTimeoutError::Disconnected) => None,
                        }
                    };

                    match block_opt {
                        Some(block) => {
                            if worker_id >= ctrl.active_workers.load(Ordering::Relaxed) {
                                if job_tx_worker.send(block).is_err() {
                                    break;
                                }
                                continue;
                            }
                            // Update queue depth on block take
                            let q_depth = queue_depth_clone.fetch_sub(1, Ordering::Relaxed) - 1;
                            let _ = tx.send(ProgressEvent::StreamProgress {
                                bytes_read: bytes_read_clone.load(Ordering::Relaxed),
                                bytes_written: bytes_written_clone.load(Ordering::Relaxed),
                                queue_depth: q_depth,
                            });
                            let _ = tx.send(ProgressEvent::WorkerStatus {
                                worker_id,
                                status: "BUSY",
                                current_chunk: Some(block.seq_num),
                            });
                            let _ = tx.send(ProgressEvent::ChunkAssigned {
                                worker_id,
                                seq_num: block.seq_num,
                            });

                            crate::log_verbose!(
                                "Worker {} compressing block {}",
                                worker_id,
                                block.seq_num
                            );
                            let tx_inner = tx.clone();
                            let total_bytes = block.data.len();
                            let seq_num = block.seq_num;
                            let processed = Arc::new(AtomicUsize::new(0));
                            let processed_callback = processed.clone();
                            let compressed = compressor.compress_with_progress(
                                &block.data,
                                &move |input_bytes, output_bytes, duration| {
                                    let current = processed_callback
                                        .fetch_add(input_bytes, Ordering::Relaxed)
                                        + input_bytes;
                                    let _ = tx_inner.send(ProgressEvent::WorkerChunkProgress {
                                        worker_id,
                                        seq_num,
                                        bytes_processed: current.min(total_bytes),
                                        bytes_written: output_bytes,
                                        total_bytes,
                                        finalized: false,
                                    });
                                    let _ =
                                        tx_inner.send(ProgressEvent::AvgCompressionTime(duration));
                                },
                                &ctrl,
                            );

                            let compressed_len = compressed.as_ref().map(|v| v.len()).unwrap_or(0);
                            let _ = tx.send(ProgressEvent::WorkerChunkProgress {
                                worker_id,
                                seq_num: block.seq_num,
                                bytes_processed: block.data.len(),
                                bytes_written: compressed_len,
                                total_bytes: block.data.len(),
                                finalized: true,
                            });
                            crate::log_verbose!(
                                "Worker {} finished block {}: {} -> {} bytes",
                                worker_id,
                                block.seq_num,
                                block.data.len(),
                                compressed_len
                            );

                            let _ = tx.send(ProgressEvent::WorkerStatus {
                                worker_id,
                                status: "HOLD",
                                current_chunk: Some(block.seq_num),
                            });

                            let result = CompressedBlock {
                                worker_id,
                                seq_num: block.seq_num,
                                data: compressed,
                            };
                            if result_tx.send(result).is_err() {
                                break;
                            }

                            let _ = tx.send(ProgressEvent::WorkerStatus {
                                worker_id,
                                status: "IDLE",
                                current_chunk: None,
                            });
                        }
                        None => break,
                    }
                }
                crate::log_verbose!("Worker thread {} exiting", worker_id);
            });
            workers.push(handle);
        }

        // Drop the local result_tx so that result_rx closes when all workers exit
        drop(result_tx);

        // Spawn reader thread
        let tx_reader = sender.clone();
        let ctrl_reader = controller.clone();
        let bytes_read_reader = bytes_read.clone();
        let bytes_written_reader = bytes_written.clone();
        let queue_depth_reader = queue_depth.clone();
        let reader_done_reader = reader_done.clone();

        let reader_handle = s.spawn(move || -> Result<(), ZipError> {
            crate::log_verbose!("Reader thread started with dynamic block sizing");
            let mut seq_num = 0u64;

            loop {
                if ctrl_reader.is_aborted.load(Ordering::Relaxed) {
                    break;
                }
                // Check pause status in reader thread
                while ctrl_reader.is_paused.load(Ordering::Relaxed) {
                    if ctrl_reader.is_aborted.load(Ordering::Relaxed) {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                if ctrl_reader.is_aborted.load(Ordering::Relaxed) {
                    break;
                }

                let chunk_size = ctrl_reader.chunk_size.load(Ordering::Acquire);
                let mut buffer = vec![0u8; chunk_size];
                let mut bytes_read_so_far = 0;
                while bytes_read_so_far < chunk_size {
                    if ctrl_reader.is_aborted.load(Ordering::Relaxed) {
                        break;
                    }
                    let n = match input.read(&mut buffer[bytes_read_so_far..]) {
                        Ok(n) => n,
                        Err(error) => {
                            reader_done_reader.store(true, Ordering::Relaxed);
                            return Err(ZipError::Io(error));
                        }
                    };
                    if n == 0 {
                        break;
                    }
                    bytes_read_so_far += n;
                }

                if bytes_read_so_far == 0 {
                    break;
                }

                // Update bytes read, queue depth
                let total_r = bytes_read_reader.fetch_add(bytes_read_so_far, Ordering::Relaxed)
                    + bytes_read_so_far;
                let q_depth = queue_depth_reader.fetch_add(1, Ordering::Relaxed) + 1;
                let _ = tx_reader.send(ProgressEvent::StreamProgress {
                    bytes_read: total_r,
                    bytes_written: bytes_written_reader.load(Ordering::Relaxed),
                    queue_depth: q_depth,
                });

                crate::log_verbose!(
                    "Reader queued block {} ({} bytes)",
                    seq_num,
                    bytes_read_so_far
                );
                buffer.truncate(bytes_read_so_far);
                let block = Block {
                    seq_num,
                    data: buffer,
                };
                let _ = tx_reader.send(ProgressEvent::ChunkQueued {
                    seq_num,
                    bytes: bytes_read_so_far,
                });
                seq_num += 1;

                if job_tx.send(block).is_err() {
                    break; // Workers shut down
                }
            }
            reader_done_reader.store(true, Ordering::Relaxed);
            crate::log_verbose!("Reader thread reached EOF and exiting");
            Ok(())
        });

        // Writer logic (runs on main thread within the scope)
        let tx_writer = sender.clone();
        let ctrl_writer = controller.clone();
        let bytes_read_writer = bytes_read.clone();
        let bytes_written_writer = bytes_written.clone();
        let queue_depth_writer = queue_depth.clone();
        let mut pending_blocks = BTreeMap::new();
        let mut next_seq_num = 0u64;
        let mut compression_error = None;

        for result in result_rx {
            if ctrl_writer.is_aborted.load(Ordering::Relaxed) {
                break;
            }
            match result.data {
                Ok(compressed_data) => {
                    pending_blocks.insert(result.seq_num, compressed_data);
                    let _ = tx_writer.send(ProgressEvent::ChunkPending {
                        worker_id: result.worker_id,
                        seq_num: result.seq_num,
                    });
                }
                Err(e) => {
                    crate::log_verbose!(
                        "Writer encountered compression error on block {}",
                        result.seq_num
                    );
                    compression_error = Some(e);
                    break; // Abort on first error
                }
            }

            while let Some(data) = pending_blocks.remove(&next_seq_num) {
                crate::log_verbose!(
                    "Writer flushing block {} ({} bytes) to output",
                    next_seq_num,
                    data.len()
                );
                let total_w =
                    bytes_written_writer.fetch_add(data.len(), Ordering::Relaxed) + data.len();
                let _ = tx_writer.send(ProgressEvent::StreamProgress {
                    bytes_read: bytes_read_writer.load(Ordering::Relaxed),
                    bytes_written: total_w,
                    queue_depth: queue_depth_writer.load(Ordering::Relaxed),
                });
                output.write_all(&data)?;
                let _ = tx_writer.send(ProgressEvent::ChunkWritten {
                    seq_num: next_seq_num,
                });
                next_seq_num += 1;
            }
        }

        // Join reader thread and check for errors
        let reader_join = reader_handle.join();
        if let Err(panic_payload) = reader_join {
            return Err(ZipError::Compression(format!(
                "Reader thread panicked: {:?}",
                panic_payload
            )));
        }
        reader_join.unwrap()?;

        if let Some(err) = compression_error {
            return Err(err);
        }

        while let Some(data) = pending_blocks.remove(&next_seq_num) {
            crate::log_verbose!(
                "Writer flushing final block {} ({} bytes) to output",
                next_seq_num,
                data.len()
            );
            let total_w =
                bytes_written_writer.fetch_add(data.len(), Ordering::Relaxed) + data.len();
            let _ = tx_writer.send(ProgressEvent::StreamProgress {
                bytes_read: bytes_read_writer.load(Ordering::Relaxed),
                bytes_written: total_w,
                queue_depth: queue_depth_writer.load(Ordering::Relaxed),
            });
            output.write_all(&data)?;
            let _ = tx_writer.send(ProgressEvent::ChunkWritten {
                seq_num: next_seq_num,
            });
            next_seq_num += 1;
        }

        Ok(())
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
