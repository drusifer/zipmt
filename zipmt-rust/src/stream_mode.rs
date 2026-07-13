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
) -> Result<(), ZipError> {
    let pool_size = if num_threads > 0 { num_threads } else { num_threads_default() };

    // Setup channels with backpressure bounds
    let (job_tx, job_rx) = sync_channel::<Block>(pool_size * 2);
    let (result_tx, result_rx) = channel::<CompressedBlock>();

    let job_rx = Arc::new(Mutex::new(job_rx));

    // Use scoped threads to safely pass non-static references across thread boundaries
    let scope_res = thread::scope(|s| {
        let mut workers = Vec::new();

        // Spawn worker threads
        for _ in 0..pool_size {
            let job_rx = Arc::clone(&job_rx);
            let result_tx = result_tx.clone();
            
            let handle = s.spawn(move || {
                loop {
                    let block_opt = {
                        let rx_guard = job_rx.lock().unwrap();
                        rx_guard.recv().ok()
                    };

                    match block_opt {
                        Some(block) => {
                            let compressed = compressor.compress(&block.data);
                            let result = CompressedBlock {
                                seq_num: block.seq_num,
                                data: compressed,
                            };
                            if result_tx.send(result).is_err() {
                                break;
                            }
                        }
                        None => break,
                    }
                }
            });
            workers.push(handle);
        }

        // Drop the local result_tx so that result_rx closes when all workers exit
        drop(result_tx);

        // Spawn reader thread
        let reader_handle = s.spawn(move || -> Result<(), ZipError> {
            let mut buffer = vec![0u8; BLOCK_SIZE];
            let mut seq_num = 0u64;

            loop {
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

                let block = Block {
                    seq_num,
                    data: buffer[..bytes_read].to_vec(),
                };
                seq_num += 1;

                if job_tx.send(block).is_err() {
                    break; // Workers shut down
                }
            }
            Ok(())
        });

        // Writer logic (runs on main thread within the scope)
        let mut pending_blocks = BTreeMap::new();
        let mut next_seq_num = 0u64;
        let mut compression_error = None;

        for result in result_rx {
            match result.data {
                Ok(compressed_data) => {
                    pending_blocks.insert(result.seq_num, compressed_data);
                }
                Err(e) => {
                    compression_error = Some(e);
                    break; // Abort on first error
                }
            }

            while let Some(data) = pending_blocks.remove(&next_seq_num) {
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
            output.write_all(&data)?;
            next_seq_num += 1;
        }

        Ok(())
    });

    scope_res?;
    output.flush()?;
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
        let result = compress_stream(&mut input_cursor, &mut output_buffer, &compressor, 4);
        assert!(result.is_ok(), "Stream mode compression failed: {:?}", result.err());

        // Decompress and verify
        let mut decoder = flate2::read::MultiGzDecoder::new(&output_buffer[..]);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed).unwrap();

        assert_eq!(original_data.to_vec(), decompressed, "Decompressed stream mismatch");
    }
}
