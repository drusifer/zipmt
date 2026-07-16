use crate::pipeline::PipelineController;
use bzip2::read::BzDecoder;
use bzip2::write::BzEncoder;
use flate2::Compression;
use flate2::read::MultiGzDecoder;
use flate2::write::GzEncoder;
use std::io::{Read, Write};
use std::sync::atomic::Ordering;
use xz2::read::XzDecoder;
use xz2::write::XzEncoder;

#[derive(Debug)]
pub enum ZipError {
    Io(std::io::Error),
    Compression(String),
    Verification(String),
}

impl std::fmt::Display for ZipError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ZipError::Io(err) => write!(f, "IO Error: {}", err),
            ZipError::Compression(msg) => write!(f, "Compression Error: {}", msg),
            ZipError::Verification(msg) => write!(f, "Verification Failed: {}", msg),
        }
    }
}

impl std::error::Error for ZipError {}

impl Clone for ZipError {
    fn clone(&self) -> Self {
        match self {
            ZipError::Io(err) => ZipError::Io(std::io::Error::new(err.kind(), err.to_string())),
            ZipError::Compression(msg) => ZipError::Compression(msg.clone()),
            ZipError::Verification(msg) => ZipError::Verification(msg.clone()),
        }
    }
}

impl From<std::io::Error> for ZipError {
    fn from(err: std::io::Error) -> Self {
        ZipError::Io(err)
    }
}

pub trait Compressor: Send + Sync {
    /// Compresses input bytes.
    fn compress(&self, input: &[u8]) -> Result<Vec<u8>, ZipError> {
        self.compress_with_progress(input, &|_, _| {}, &PipelineController::new(6))
    }

    /// Compresses input bytes with a progress callback.
    fn compress_with_progress(
        &self,
        input: &[u8],
        on_progress: &(dyn Fn(usize, std::time::Duration) + Send + Sync),
        controller: &PipelineController,
    ) -> Result<Vec<u8>, ZipError>;

    /// Decompresses input bytes to verify integrity, returning an error on corruption.
    fn verify(&self, input: &[u8]) -> Result<(), ZipError>;
}

fn check_throttle(controller: &PipelineController) {
    let delay = controller.throttle_delay_ms.load(Ordering::Relaxed);
    if delay > 0 {
        std::thread::sleep(std::time::Duration::from_millis(delay));
    }
}

/// Gzip compressor adapter.
pub struct GzipCompressor {
    pub level: u32,
}

impl Compressor for GzipCompressor {
    fn compress_with_progress(
        &self,
        input: &[u8],
        on_progress: &(dyn Fn(usize, std::time::Duration) + Send + Sync),
        controller: &PipelineController,
    ) -> Result<Vec<u8>, ZipError> {
        let level = controller.compression_level.load(Ordering::Relaxed);
        let mut encoder = GzEncoder::new(Vec::new(), Compression::new(level));
        let chunk_size = 64 * 1024;
        for chunk in input.chunks(chunk_size) {
            if controller.is_aborted.load(Ordering::Relaxed) {
                return Err(ZipError::Compression("Aborted".into()));
            }
            while controller.is_paused.load(Ordering::Relaxed) {
                if controller.is_aborted.load(Ordering::Relaxed) {
                    return Err(ZipError::Compression("Aborted".into()));
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            check_throttle(controller);
            let start = std::time::Instant::now();
            encoder.write_all(chunk)?;
            let duration = start.elapsed();
            on_progress(chunk.len(), duration);
        }
        let result = encoder.finish()?;
        Ok(result)
    }

    fn verify(&self, input: &[u8]) -> Result<(), ZipError> {
        let mut decoder = MultiGzDecoder::new(input);
        let mut buffer = Vec::new();
        decoder
            .read_to_end(&mut buffer)
            .map_err(|e| ZipError::Verification(format!("Gzip decompression failed: {}", e)))?;
        Ok(())
    }
}

/// Bzip2 compressor adapter.
pub struct Bzip2Compressor {
    pub level: u32,
}

impl Compressor for Bzip2Compressor {
    fn compress_with_progress(
        &self,
        input: &[u8],
        on_progress: &(dyn Fn(usize, std::time::Duration) + Send + Sync),
        controller: &PipelineController,
    ) -> Result<Vec<u8>, ZipError> {
        let level = controller.compression_level.load(Ordering::Relaxed);
        let mut encoder = BzEncoder::new(Vec::new(), bzip2::Compression::new(level));
        let chunk_size = 64 * 1024;
        for chunk in input.chunks(chunk_size) {
            if controller.is_aborted.load(Ordering::Relaxed) {
                return Err(ZipError::Compression("Aborted".into()));
            }
            while controller.is_paused.load(Ordering::Relaxed) {
                if controller.is_aborted.load(Ordering::Relaxed) {
                    return Err(ZipError::Compression("Aborted".into()));
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            check_throttle(controller);
            let start = std::time::Instant::now();
            encoder.write_all(chunk)?;
            let duration = start.elapsed();
            on_progress(chunk.len(), duration);
        }
        let result = encoder.finish()?;
        Ok(result)
    }

    fn verify(&self, input: &[u8]) -> Result<(), ZipError> {
        let mut decoder = BzDecoder::new(input);
        let mut buffer = Vec::new();
        decoder
            .read_to_end(&mut buffer)
            .map_err(|e| ZipError::Verification(format!("Bzip2 decompression failed: {}", e)))?;
        Ok(())
    }
}

/// Xz compressor adapter.
pub struct XzCompressor {
    pub level: u32,
}

impl Compressor for XzCompressor {
    fn compress_with_progress(
        &self,
        input: &[u8],
        on_progress: &(dyn Fn(usize, std::time::Duration) + Send + Sync),
        controller: &PipelineController,
    ) -> Result<Vec<u8>, ZipError> {
        let level = controller.compression_level.load(Ordering::Relaxed);
        let mut encoder = XzEncoder::new(Vec::new(), level);
        let chunk_size = 64 * 1024;
        for chunk in input.chunks(chunk_size) {
            if controller.is_aborted.load(Ordering::Relaxed) {
                return Err(ZipError::Compression("Aborted".into()));
            }
            while controller.is_paused.load(Ordering::Relaxed) {
                if controller.is_aborted.load(Ordering::Relaxed) {
                    return Err(ZipError::Compression("Aborted".into()));
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            check_throttle(controller);
            let start = std::time::Instant::now();
            encoder.write_all(chunk)?;
            let duration = start.elapsed();
            on_progress(chunk.len(), duration);
        }
        let result = encoder.finish()?;
        Ok(result)
    }

    fn verify(&self, input: &[u8]) -> Result<(), ZipError> {
        let mut decoder = XzDecoder::new_multi_decoder(input);
        let mut buffer = Vec::new();
        decoder
            .read_to_end(&mut buffer)
            .map_err(|e| ZipError::Verification(format!("XZ decompression failed: {}", e)))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_compressor_behavior(compressor: &dyn Compressor) {
        let original_data = b"Hello, this is a test string to verify the compression and decompression loops in Rust!";

        // Test compression
        let compressed = compressor.compress(original_data);
        assert!(compressed.is_ok(), "Compression failed");
        let compressed_data = compressed.unwrap();
        assert!(!compressed_data.is_empty(), "Compressed data is empty");
        assert_ne!(
            original_data.to_vec(),
            compressed_data,
            "Data was not compressed/modified"
        );

        // Test verification passes on valid data
        let verify_result = compressor.verify(&compressed_data);
        assert!(
            verify_result.is_ok(),
            "Verification failed on valid data: {:?}",
            verify_result.err()
        );

        // Test verification fails on corrupt data
        let mut corrupt_data = compressed_data.clone();
        if corrupt_data.len() > 10 {
            // Corrupt middle bytes
            for i in (corrupt_data.len() / 2)..(corrupt_data.len() / 2 + 5) {
                corrupt_data[i] ^= 0xFF;
            }
        } else {
            corrupt_data[0] ^= 0xFF;
        }
        let verify_corrupt = compressor.verify(&corrupt_data);
        assert!(
            verify_corrupt.is_err(),
            "Verification succeeded on corrupted data"
        );
    }

    #[test]
    fn test_gzip_compressor() {
        test_compressor_behavior(&GzipCompressor { level: 6 });
    }

    #[test]
    fn test_bzip2_compressor() {
        test_compressor_behavior(&Bzip2Compressor { level: 6 });
    }

    #[test]
    fn test_xz_compressor() {
        test_compressor_behavior(&XzCompressor { level: 6 });
    }
}
