use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{sync_channel, channel};
use std::thread;
use crate::compressor::{Compressor, ZipError};

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
    tui_state: Option<Arc<Mutex<crate::tui::TuiState>>>,
) -> Result<(), ZipError> {
    let pool_size = if num_threads > 0 { num_threads } else { num_threads_default() };
    crate::log_verbose!("Starting pipeline stream compression with pool size: {}", pool_size);

    // Setup channels with backpressure bounds
    let (job_tx, job_rx) = sync_channel::<Block>(pool_size * 2);
    let (result_tx, result_rx) = channel::<CompressedBlock>();

    let job_rx = Arc::new(Mutex::new(job_rx));

    // Use scoped threads to safely pass non-static references across thread boundaries
    let tui_state_ref1 = tui_state.clone();
    let tui_state_ref2 = tui_state.clone();
    let tui_state_ref3 = tui_state.clone();
    let scope_res = thread::scope(|s| {
        let mut workers = Vec::new();

        // Spawn worker threads
        for worker_id in 0..pool_size {
            let job_rx = Arc::clone(&job_rx);
            let result_tx = result_tx.clone();
            let tui_ref = tui_state_ref1.clone();
            
            let handle = s.spawn(move || {
                crate::log_verbose!("Worker thread {} started", worker_id);
                loop {
                    let block_opt = {
                        let rx_guard = job_rx.lock().unwrap();
                        rx_guard.recv().ok()
                    };

                    match block_opt {
                        Some(block) => {
                            // Update queue depth on block take
                            if let Some(ref tui) = tui_ref {
                                let mut guard = tui.lock().unwrap();
                                if guard.queue_depth > 0 {
                                    guard.queue_depth -= 1;
                                }
                                guard.input_queue.retain(|&x| x != block.seq_num);
                                if worker_id < guard.workers.len() {
                                    guard.workers[worker_id].status = "BUSY";
                                    guard.workers[worker_id].current_chunk = Some(block.seq_num);
                                }
                            }
                            crate::log_verbose!("Worker {} compressing block {}", worker_id, block.seq_num);
                            let compressed = compressor.compress(&block.data);
                            let compressed_len = compressed.as_ref().map(|v| v.len()).unwrap_or(0);
                            crate::log_verbose!(
                                "Worker {} finished block {}: {} -> {} bytes",
                                worker_id,
                                block.seq_num,
                                block.data.len(),
                                compressed_len
                            );
                            if let Some(ref tui) = tui_ref {
                                let mut guard = tui.lock().unwrap();
                                if worker_id < guard.workers.len() {
                                    guard.workers[worker_id].status = "HOLD";
                                }
                            }
                            let result = CompressedBlock {
                                seq_num: block.seq_num,
                                data: compressed,
                            };
                            if result_tx.send(result).is_err() {
                                break;
                            }
                            if let Some(ref tui) = tui_ref {
                                let mut guard = tui.lock().unwrap();
                                if worker_id < guard.workers.len() {
                                    guard.workers[worker_id].status = "IDLE";
                                    guard.workers[worker_id].current_chunk = None;
                                }
                            }
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
        let tui_ref_reader = tui_state_ref2.clone();
        let reader_handle = s.spawn(move || -> Result<(), ZipError> {
            crate::log_verbose!("Reader thread started. Buffer block size: {}MB", BLOCK_SIZE / (1024 * 1024));
            let mut buffer = vec![0u8; BLOCK_SIZE];
            let mut seq_num = 0u64;

            loop {
                // Check pause status in reader thread
                while crate::IS_PAUSED.load(std::sync::atomic::Ordering::Relaxed) {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }

                let mut bytes_read = 0;
                while bytes_read < BLOCK_SIZE {
                    let n = input.read(&mut buffer[bytes_read..])?;
                    if n == 0 {
                        break;
                    }
                    bytes_read += n;
                }

                if bytes_read == 0 {
                    break;
                }

                // Update bytes read, queue depth and input queue
                if let Some(ref tui) = tui_ref_reader {
                    let mut guard = tui.lock().unwrap();
                    guard.bytes_read += bytes_read;
                    guard.queue_depth += 1;
                    guard.input_queue.push(seq_num);
                }

                crate::log_verbose!("Reader queued block {} ({} bytes)", seq_num, bytes_read);
                let block = Block {
                    seq_num,
                    data: buffer[..bytes_read].to_vec(),
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
        let tui_ref_writer = tui_state_ref3.clone();
        let mut pending_blocks = BTreeMap::new();
        let mut next_seq_num = 0u64;
        let mut compression_error = None;

        for result in result_rx {
            match result.data {
                Ok(compressed_data) => {
                    if let Some(ref tui) = tui_ref_writer {
                        let mut guard = tui.lock().unwrap();
                        guard.output_buffer.push(result.seq_num);
                        guard.output_buffer.sort_unstable();
                        guard.next_expected_seq = next_seq_num;
                    }
                    pending_blocks.insert(result.seq_num, compressed_data);
                }
                Err(e) => {
                    crate::log_verbose!("Writer encountered compression error on block {}", result.seq_num);
                    compression_error = Some(e);
                    break; // Abort on first error
                }
            }

            while let Some(data) = pending_blocks.remove(&next_seq_num) {
                crate::log_verbose!("Writer flushing block {} ({} bytes) to output", next_seq_num, data.len());
                // Update bytes written
                if let Some(ref tui) = tui_ref_writer {
                    let mut guard = tui.lock().unwrap();
                    guard.bytes_written += data.len();
                    guard.output_buffer.retain(|&x| x != next_seq_num);
                    guard.next_expected_seq = next_seq_num + 1;
                }
                output.write_all(&data)?;
                next_seq_num += 1;
            }
        }

        // Join reader thread and check for errors
        let reader_join = reader_handle.join();
        if let Err(panic_payload) = reader_join {
            return Err(ZipError::Compression(format!("Reader thread panicked: {:?}", panic_payload)));
        }
        reader_join.unwrap()?;

        if let Some(err) = compression_error {
            return Err(err);
        }

        while let Some(data) = pending_blocks.remove(&next_seq_num) {
            crate::log_verbose!("Writer flushing final block {} ({} bytes) to output", next_seq_num, data.len());
            if let Some(ref tui) = tui_ref_writer {
                let mut guard = tui.lock().unwrap();
                guard.output_buffer.retain(|&x| x != next_seq_num);
                guard.next_expected_seq = next_seq_num + 1;
            }
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
        let original_data = b"This is some pipeline stream data that we want to compress. ".repeat(200000); // ~12MB
        let mut input_cursor = Cursor::new(original_data.clone());
        let mut output_buffer = Vec::new();

        let compressor = GzipCompressor;
        let result = compress_stream(&mut input_cursor, &mut output_buffer, &compressor, 4, None);
        assert!(result.is_ok(), "Stream mode compression failed: {:?}", result.err());

        // Decompress and verify
        let mut decoder = flate2::read::MultiGzDecoder::new(&output_buffer[..]);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed).unwrap();

        assert_eq!(original_data.to_vec(), decompressed, "Decompressed stream mismatch");
    }
}
