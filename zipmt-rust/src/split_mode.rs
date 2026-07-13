use std::fs::File;
use std::io::Write;
use std::path::Path;
use rayon::prelude::*;
use crate::compressor::{Compressor, ZipError};

/// Compresses a file in Split Mode using Rayon for parallel chunk processing.
pub fn compress_file(
    input_path: &Path,
    output_path: &Path,
    compressor: &dyn Compressor,
    num_threads: usize,
) -> Result<(), ZipError> {
    // Read entire input file
    let input_data = std::fs::read(input_path)?;
    let input_len = input_data.len();

    if input_len == 0 {
        // Handle empty file: just compress empty buffer and write it
        let compressed = compressor.compress(&[])?;
        let mut out_file = File::create(output_path)?;
        out_file.write_all(&compressed)?;
        return Ok(());
    }

    // Determine chunk size based on thread count
    let chunks_count = if num_threads > 0 { num_threads } else { rayon::current_num_threads() };
    let chunk_size = (input_len + chunks_count - 1) / chunks_count;

    // Collect chunks as slices
    let chunks: Vec<&[u8]> = input_data.chunks(chunk_size).collect();

    // Setup rayon thread pool override if num_threads is specified
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(chunks_count)
        .build()
        .map_err(|e| ZipError::Compression(e.to_string()))?;

    // Compress chunks concurrently
    let compressed_chunks: Result<Vec<Vec<u8>>, ZipError> = pool.install(|| {
        chunks
            .into_par_iter()
            .map(|chunk| compressor.compress(chunk))
            .collect()
    });

    let compressed_chunks = compressed_chunks?;

    // Write all compressed chunks sequentially to output
    let mut out_file = File::create(output_path)?;
    for chunk in compressed_chunks {
        out_file.write_all(&chunk)?;
    }

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

        let compressor = GzipCompressor;
        let result = compress_file(&input_path, &output_path, &compressor, 4);
        assert!(result.is_ok(), "Split mode compression failed");

        // Verify we can decompress the output and it matches
        let compressed_content = std::fs::read(&output_path).unwrap();
        let mut decoder = flate2::read::MultiGzDecoder::new(&compressed_content[..]);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed).unwrap();

        assert_eq!(original_content, decompressed, "Decompressed content mismatch");
    }
}
