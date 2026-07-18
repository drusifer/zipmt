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
        self.compress_with_progress(
            input,
            &|_, _, _| {},
            &PipelineController::new(crate::DEFAULT_COMPRESSION_LEVEL),
        )
    }

    /// Compresses input bytes with a progress callback.
    fn compress_with_progress(
        &self,
        input: &[u8],
        on_progress: &(dyn Fn(usize, usize, std::time::Duration) + Send + Sync),
        controller: &PipelineController,
    ) -> Result<Vec<u8>, ZipError> {
        let mut output = Vec::new();
        self.compress_slice_to_writer(input, &mut output, on_progress, controller)?;
        Ok(output)
    }

    /// Compresses an existing in-memory chunk directly, without copying it
    /// through an intermediate read buffer.
    fn compress_slice_to_writer(
        &self,
        input: &[u8],
        writer: &mut dyn Write,
        on_progress: &(dyn Fn(usize, usize, std::time::Duration) + Send + Sync),
        controller: &PipelineController,
    ) -> Result<usize, ZipError>;

    /// Compresses a bounded reader into a writer without buffering the full stream.
    fn compress_reader_to_writer(
        &self,
        reader: &mut dyn Read,
        writer: &mut dyn Write,
        on_progress: &(dyn Fn(usize, usize, std::time::Duration) + Send + Sync),
        controller: &PipelineController,
    ) -> Result<usize, ZipError>;

    /// Decompresses input bytes to verify integrity, returning an error on corruption.
    fn verify(&self, input: &[u8]) -> Result<(), ZipError>;
}

struct CountingWriter<'a> {
    inner: &'a mut dyn Write,
    bytes_written: usize,
}

impl Write for CountingWriter<'_> {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let written = self.inner.write(buffer)?;
        self.bytes_written = self.bytes_written.saturating_add(written);
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

fn check_throttle(controller: &PipelineController) {
    let delay = controller.throttle_delay_ms.load(Ordering::Relaxed);
    if delay > 0 {
        std::thread::sleep(std::time::Duration::from_millis(delay));
    }
}

fn check_control_state(controller: &PipelineController) -> Result<(), ZipError> {
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
    Ok(())
}

/// Gzip compressor adapter.
pub struct GzipCompressor {
    pub level: u32,
}

impl Compressor for GzipCompressor {
    fn compress_slice_to_writer(
        &self,
        input: &[u8],
        writer: &mut dyn Write,
        on_progress: &(dyn Fn(usize, usize, std::time::Duration) + Send + Sync),
        controller: &PipelineController,
    ) -> Result<usize, ZipError> {
        let level = controller.compression_level.load(Ordering::Relaxed);
        let counting_writer = CountingWriter {
            inner: writer,
            bytes_written: 0,
        };
        let mut encoder = GzEncoder::new(counting_writer, Compression::new(level));
        check_control_state(controller)?;
        let start = std::time::Instant::now();
        encoder.write_all(input)?;
        on_progress(
            input.len(),
            encoder.get_ref().bytes_written,
            start.elapsed(),
        );
        Ok(encoder.finish()?.bytes_written)
    }

    fn compress_reader_to_writer(
        &self,
        reader: &mut dyn Read,
        writer: &mut dyn Write,
        on_progress: &(dyn Fn(usize, usize, std::time::Duration) + Send + Sync),
        controller: &PipelineController,
    ) -> Result<usize, ZipError> {
        let level = controller.compression_level.load(Ordering::Relaxed);
        let counting_writer = CountingWriter {
            inner: writer,
            bytes_written: 0,
        };
        let mut encoder = GzEncoder::new(counting_writer, Compression::new(level));
        let mut buffer = vec![0_u8; controller.chunk_size.load(Ordering::Acquire)];
        loop {
            let requested = controller.chunk_size.load(Ordering::Acquire);
            if buffer.len() != requested {
                buffer = vec![0_u8; requested];
            }
            let bytes_read = reader.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            check_control_state(controller)?;
            let start = std::time::Instant::now();
            encoder.write_all(&buffer[..bytes_read])?;
            let duration = start.elapsed();
            on_progress(bytes_read, encoder.get_ref().bytes_written, duration);
        }
        let result = encoder.finish()?;
        Ok(result.bytes_written)
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
    fn compress_slice_to_writer(
        &self,
        input: &[u8],
        writer: &mut dyn Write,
        on_progress: &(dyn Fn(usize, usize, std::time::Duration) + Send + Sync),
        controller: &PipelineController,
    ) -> Result<usize, ZipError> {
        let level = controller.compression_level.load(Ordering::Relaxed);
        let counting_writer = CountingWriter {
            inner: writer,
            bytes_written: 0,
        };
        let mut encoder = BzEncoder::new(counting_writer, bzip2::Compression::new(level));
        check_control_state(controller)?;
        let start = std::time::Instant::now();
        encoder.write_all(input)?;
        on_progress(
            input.len(),
            encoder.get_ref().bytes_written,
            start.elapsed(),
        );
        Ok(encoder.finish()?.bytes_written)
    }

    fn compress_reader_to_writer(
        &self,
        reader: &mut dyn Read,
        writer: &mut dyn Write,
        on_progress: &(dyn Fn(usize, usize, std::time::Duration) + Send + Sync),
        controller: &PipelineController,
    ) -> Result<usize, ZipError> {
        let level = controller.compression_level.load(Ordering::Relaxed);
        let counting_writer = CountingWriter {
            inner: writer,
            bytes_written: 0,
        };
        let mut encoder = BzEncoder::new(counting_writer, bzip2::Compression::new(level));
        let mut buffer = vec![0_u8; controller.chunk_size.load(Ordering::Acquire)];
        loop {
            let requested = controller.chunk_size.load(Ordering::Acquire);
            if buffer.len() != requested {
                buffer = vec![0_u8; requested];
            }
            let bytes_read = reader.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            check_control_state(controller)?;
            let start = std::time::Instant::now();
            encoder.write_all(&buffer[..bytes_read])?;
            let duration = start.elapsed();
            on_progress(bytes_read, encoder.get_ref().bytes_written, duration);
        }
        let result = encoder.finish()?;
        Ok(result.bytes_written)
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
    fn compress_slice_to_writer(
        &self,
        input: &[u8],
        writer: &mut dyn Write,
        on_progress: &(dyn Fn(usize, usize, std::time::Duration) + Send + Sync),
        controller: &PipelineController,
    ) -> Result<usize, ZipError> {
        let level = controller.compression_level.load(Ordering::Relaxed);
        let counting_writer = CountingWriter {
            inner: writer,
            bytes_written: 0,
        };
        let mut encoder = XzEncoder::new(counting_writer, level);
        check_control_state(controller)?;
        let start = std::time::Instant::now();
        encoder.write_all(input)?;
        on_progress(
            input.len(),
            encoder.get_ref().bytes_written,
            start.elapsed(),
        );
        Ok(encoder.finish()?.bytes_written)
    }

    fn compress_reader_to_writer(
        &self,
        reader: &mut dyn Read,
        writer: &mut dyn Write,
        on_progress: &(dyn Fn(usize, usize, std::time::Duration) + Send + Sync),
        controller: &PipelineController,
    ) -> Result<usize, ZipError> {
        let level = controller.compression_level.load(Ordering::Relaxed);
        let counting_writer = CountingWriter {
            inner: writer,
            bytes_written: 0,
        };
        let mut encoder = XzEncoder::new(counting_writer, level);
        let mut buffer = vec![0_u8; controller.chunk_size.load(Ordering::Acquire)];
        loop {
            let requested = controller.chunk_size.load(Ordering::Acquire);
            if buffer.len() != requested {
                buffer = vec![0_u8; requested];
            }
            let bytes_read = reader.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            check_control_state(controller)?;
            let start = std::time::Instant::now();
            encoder.write_all(&buffer[..bytes_read])?;
            let duration = start.elapsed();
            on_progress(bytes_read, encoder.get_ref().bytes_written, duration);
        }
        let result = encoder.finish()?;
        Ok(result.bytes_written)
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
    use std::io::Cursor;
    use std::sync::{Arc, Mutex};

    struct ResizingReader {
        inner: Cursor<Vec<u8>>,
        controller: PipelineController,
        read_sizes: Arc<Mutex<Vec<usize>>>,
        changed: bool,
    }

    impl Read for ResizingReader {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            self.read_sizes.lock().unwrap().push(buffer.len());
            let read = self.inner.read(buffer)?;
            if read > 0 && !self.changed {
                assert!(self.controller.update_chunk_size(128 * 1024));
                self.changed = true;
            }
            Ok(read)
        }
    }

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

    #[test]
    fn stream_chunk_is_fed_directly_to_encoder() {
        let input = vec![b'x'; 256 * 1024];
        let calls = Arc::new(Mutex::new(Vec::new()));
        let callback_calls = calls.clone();
        let controller = PipelineController::new(6);
        let compressed = GzipCompressor { level: 6 }
            .compress_with_progress(
                &input,
                &move |input_bytes, _, _| callback_calls.lock().unwrap().push(input_bytes),
                &controller,
            )
            .unwrap();

        assert!(!compressed.is_empty());
        assert_eq!(*calls.lock().unwrap(), vec![input.len()]);
    }

    #[test]
    fn split_reader_replaces_reusable_chunk_after_atomic_size_change() {
        let input = vec![b'y'; 300 * 1024];
        let controller = PipelineController::new(6);
        assert!(controller.update_chunk_size(64 * 1024));
        let read_sizes = Arc::new(Mutex::new(Vec::new()));
        let mut reader = ResizingReader {
            inner: Cursor::new(input),
            controller: controller.clone(),
            read_sizes: read_sizes.clone(),
            changed: false,
        };
        let mut output = Vec::new();

        GzipCompressor { level: 6 }
            .compress_reader_to_writer(&mut reader, &mut output, &|_, _, _| {}, &controller)
            .unwrap();

        let sizes = read_sizes.lock().unwrap();
        assert_eq!(sizes[0], 64 * 1024);
        assert_eq!(sizes[1], 128 * 1024);
    }
}
