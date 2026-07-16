use crate::compressor::{Compressor, ZipError};
use crate::pipeline::{PipelineController, ProgressEvent};
use rayon::prelude::*;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::Sender;

/// Compresses a file in Split Mode using Rayon for parallel chunk processing.
pub fn compress_file(
    input_path: &Path,
    output_path: &Path,
    compressor: &dyn Compressor,
    num_threads: usize,
    sender: Sender<ProgressEvent>,
    controller: &PipelineController,
) -> Result<(), ZipError> {
    // Read entire input file
    crate::log_verbose!("Reading input file: {:?}", input_path);
    let input_data = std::fs::read(input_path)?;
    let input_len = input_data.len();

    if input_len == 0 {
        crate::log_verbose!("Empty file detected. Compressing empty buffer...");
        let compressed = compressor.compress(input_data.as_slice())?;
        let mut out_file = File::create(output_path)?;
        out_file.write_all(&compressed)?;
        return Ok(());
    }

    // Determine chunk size based on thread count
    let chunks_count = if num_threads > 0 {
        num_threads
    } else {
        rayon::current_num_threads()
    };
    let chunk_size = (input_len + chunks_count - 1) / chunks_count;

    crate::log_verbose!(
        "Input size: {} bytes. Partitioning into {} chunks of target size: {} bytes",
        input_len,
        chunks_count,
        chunk_size
    );

    // Collect chunks as slices
    let chunks: Vec<&[u8]> = input_data.chunks(chunk_size).collect();

    // Send initial split progress events
    for (i, chunk) in chunks.iter().enumerate() {
        let _ = sender.send(ProgressEvent::SplitProgress {
            stripe_id: i,
            bytes_processed: 0,
            bytes_written: 0,
            total_bytes: chunk.len(),
        });
    }

    crate::log_verbose!(
        "Initializing Rayon thread pool with {} threads...",
        chunks_count
    );
    // Setup rayon thread pool override if num_threads is specified
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(chunks_count)
        .build()
        .map_err(|e| ZipError::Compression(e.to_string()))?;

    crate::log_verbose!("Compressing chunks concurrently...");
    // Compress chunks concurrently
    let sender_clone = sender.clone();
    let controller_clone = controller.clone();
    let compressed_chunks: Result<Vec<Vec<u8>>, ZipError> = pool.install(|| {
        chunks
            .into_par_iter()
            .enumerate()
            .map(|(i, chunk)| {
                let tx = sender_clone.clone();
                let ctrl = controller_clone.clone();
                let bytes_processed = Arc::new(AtomicUsize::new(0));
                let bytes_processed_clone = bytes_processed.clone();
                let tx_clone = tx.clone();
                let total_bytes = chunk.len();

                let res = compressor.compress_with_progress(
                    chunk,
                    &move |bytes, duration| {
                        let current =
                            bytes_processed_clone.fetch_add(bytes, Ordering::Relaxed) + bytes;
                        let _ = tx_clone.send(ProgressEvent::SplitProgress {
                            stripe_id: i,
                            bytes_processed: current,
                            bytes_written: 0,
                            total_bytes,
                        });
                        let _ = tx_clone.send(ProgressEvent::AvgCompressionTime(duration));
                    },
                    &ctrl,
                );

                if let Ok(ref compressed) = res {
                    let _ = tx.send(ProgressEvent::SplitProgress {
                        stripe_id: i,
                        bytes_processed: total_bytes,
                        bytes_written: compressed.len(),
                        total_bytes,
                    });
                }
                res
            })
            .collect()
    });

    let compressed_chunks = compressed_chunks?;

    crate::log_verbose!(
        "Writing {} compressed chunks sequentially to {:?}",
        compressed_chunks.len(),
        output_path
    );
    // Write all compressed chunks sequentially to output
    let mut out_file = File::create(output_path)?;
    for chunk in compressed_chunks {
        out_file.write_all(&chunk)?;
    }

    crate::log_verbose!("Compression completed successfully.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compressor::GzipCompressor;
    use std::io::Read;
    use tempfile::tempdir;

    #[test]
    fn test_split_mode_compression() {
        let dir = tempdir().unwrap();
        let input_path = dir.path().join("input.txt");
        let output_path = dir.path().join("output.txt.gz");

        let original_content = b"This is some sample text repeating. ".repeat(100);
        std::fs::write(&input_path, &original_content).unwrap();

        let compressor = GzipCompressor { level: 6 };
        let (tx, _rx) = std::sync::mpsc::channel();
        let controller = PipelineController::new(6);
        let result = compress_file(&input_path, &output_path, &compressor, 4, tx, &controller);
        assert!(result.is_ok(), "Split mode compression failed");

        // Verify we can decompress the output and it matches
        let compressed_content = std::fs::read(&output_path).unwrap();
        let mut decoder = flate2::read::MultiGzDecoder::new(&compressed_content[..]);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed).unwrap();

        assert_eq!(
            original_content, decompressed,
            "Decompressed content mismatch"
        );
    }
}
