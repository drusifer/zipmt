use crate::compressor::{Compressor, ZipError};
use crate::pipeline::{PipelineController, ProgressEvent};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Sender, channel, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread;

const BLOCK_SIZE: usize = 4 * 1024 * 1024; // 4MB

pub struct Block {
    pub seq_num: u64,
    pub data: Vec<u8>,
}

pub struct CompressedBlock {
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

    // Use scoped threads to safely pass non-static references across thread boundaries
    let scope_res = thread::scope(|s| {
        let mut workers = Vec::new();

        // Spawn worker threads
        for worker_id in 0..pool_size {
            let job_rx = Arc::clone(&job_rx);
            let result_tx = result_tx.clone();
            let tx = sender.clone();
            let ctrl = controller.clone();
            let bytes_read_clone = bytes_read.clone();
            let bytes_written_clone = bytes_written.clone();
            let queue_depth_clone = queue_depth.clone();

            let handle = s.spawn(move || {
                crate::log_verbose!("Worker thread {} started", worker_id);
                loop {
                    if ctrl.is_aborted.load(Ordering::Relaxed) {
                        break;
                    }
                    let block_opt = {
                        let rx_guard = job_rx.lock().unwrap();
                        rx_guard.recv().ok()
                    };

                    match block_opt {
                        Some(block) => {
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

                            crate::log_verbose!(
                                "Worker {} compressing block {}",
                                worker_id,
                                block.seq_num
                            );
                            let tx_inner = tx.clone();
                            let compressed = compressor.compress_with_progress(
                                &block.data,
                                &move |_, duration| {
                                    let _ =
                                        tx_inner.send(ProgressEvent::AvgCompressionTime(duration));
                                },
                                &ctrl,
                            );

                            let compressed_len = compressed.as_ref().map(|v| v.len()).unwrap_or(0);
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

        let reader_handle = s.spawn(move || -> Result<(), ZipError> {
            crate::log_verbose!(
                "Reader thread started. Buffer block size: {}MB",
                BLOCK_SIZE / (1024 * 1024)
            );
            let mut buffer = vec![0u8; BLOCK_SIZE];
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

                let mut bytes_read_so_far = 0;
                while bytes_read_so_far < BLOCK_SIZE {
                    if ctrl_reader.is_aborted.load(Ordering::Relaxed) {
                        break;
                    }
                    let n = input.read(&mut buffer[bytes_read_so_far..])?;
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
                let block = Block {
                    seq_num,
                    data: buffer[..bytes_read_so_far].to_vec(),
                };
                seq_num += 1;

                if job_tx.send(block).is_err() {
                    break; // Workers shut down
                }
            }
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

    #[test]
    fn test_stream_mode_compression() {
        let original_data =
            b"This is some pipeline stream data that we want to compress. ".repeat(200000); // ~12MB
        let mut input_cursor = Cursor::new(original_data.clone());
        let mut output_buffer = Vec::new();

        let compressor = GzipCompressor { level: 6 };
        let (tx, _rx) = std::sync::mpsc::channel();
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
    }
}
