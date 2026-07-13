use std::io::{Read, Write};
use flate2::Compression;
use flate2::write::GzEncoder;
use flate2::read::MultiGzDecoder;
use bzip2::write::BzEncoder;
use bzip2::read::BzDecoder;
use xz2::write::XzEncoder;
use xz2::read::XzDecoder;

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

impl From<std::io::Error> for ZipError {
    fn from(err: std::io::Error) -> Self {
        ZipError::Io(err)
    }
}

pub trait Compressor: Send + Sync {
    /// Compresses input bytes.
    fn compress(&self, input: &[u8]) -> Result<Vec<u8>, ZipError>;

    /// Decompresses input bytes to verify integrity, returning an error on corruption.
    fn verify(&self, input: &[u8]) -> Result<(), ZipError>;
}

/// Gzip compressor adapter.
pub struct GzipCompressor;

impl Compressor for GzipCompressor {
    fn compress(&self, input: &[u8]) -> Result<Vec<u8>, ZipError> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(input)?;
        let result = encoder.finish()?;
        Ok(result)
    }

    fn verify(&self, input: &[u8]) -> Result<(), ZipError> {
        let mut decoder = MultiGzDecoder::new(input);
        let mut buffer = Vec::new();
        decoder.read_to_end(&mut buffer).map_err(|e| {
            ZipError::Verification(format!("Gzip decompression failed: {}", e))
        })?;
        Ok(())
    }
}

/// Bzip2 compressor adapter.
pub struct Bzip2Compressor;

impl Compressor for Bzip2Compressor {
    fn compress(&self, input: &[u8]) -> Result<Vec<u8>, ZipError> {
        let mut encoder = BzEncoder::new(Vec::new(), bzip2::Compression::default());
        encoder.write_all(input)?;
        let result = encoder.finish()?;
        Ok(result)
    }

    fn verify(&self, input: &[u8]) -> Result<(), ZipError> {
        let mut decoder = BzDecoder::new(input);
        let mut buffer = Vec::new();
        decoder.read_to_end(&mut buffer).map_err(|e| {
            ZipError::Verification(format!("Bzip2 decompression failed: {}", e))
        })?;
        Ok(())
    }
}

/// Xz compressor adapter.
pub struct XzCompressor;

impl Compressor for XzCompressor {
    fn compress(&self, input: &[u8]) -> Result<Vec<u8>, ZipError> {
        // level 6 is standard default for xz
        let mut encoder = XzEncoder::new(Vec::new(), 6);
        encoder.write_all(input)?;
        let result = encoder.finish()?;
        Ok(result)
    }

    fn verify(&self, input: &[u8]) -> Result<(), ZipError> {
        let mut decoder = XzDecoder::new_multi_decoder(input);
        let mut buffer = Vec::new();
        decoder.read_to_end(&mut buffer).map_err(|e| {
            ZipError::Verification(format!("XZ decompression failed: {}", e))
        })?;
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
        assert_ne!(original_data.to_vec(), compressed_data, "Data was not compressed/modified");

        // Test verification passes on valid data
        let verify_result = compressor.verify(&compressed_data);
        assert!(verify_result.is_ok(), "Verification failed on valid data: {:?}", verify_result.err());

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
        assert!(verify_corrupt.is_err(), "Verification succeeded on corrupted data");
    }

    #[test]
    fn test_gzip_compressor() {
        test_compressor_behavior(&GzipCompressor);
    }

    #[test]
    fn test_bzip2_compressor() {
        test_compressor_behavior(&Bzip2Compressor);
    }

    #[test]
    fn test_xz_compressor() {
        test_compressor_behavior(&XzCompressor);
    }
}
