use crate::compressor::{Compressor, ZipError};
use crate::pipeline::{PipelineController, ProgressEvent, ProgressSink, SplitStage};
use rayon::prelude::*;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use tempfile::tempfile_in;

const FINAL_COPY_BUFFER_SIZE: usize = 64 * 1024;

#[derive(Clone, Copy)]
struct SliceRange {
    id: usize,
    start: u64,
    length: u64,
}

type TemporarySlice = (usize, File, usize);

fn plan_ranges(input_len: u64, requested_slices: usize) -> Vec<SliceRange> {
    if input_len == 0 {
        return vec![SliceRange {
            id: 0,
            start: 0,
            length: 0,
        }];
    }
    let slice_size = input_len.div_ceil(requested_slices as u64);
    (0..requested_slices)
        .map(|id| {
            let start = id as u64 * slice_size;
            SliceRange {
                id,
                start,
                length: slice_size.min(input_len.saturating_sub(start)),
            }
        })
        .take_while(|range| range.start < input_len)
        .collect()
}

struct SliceRuntime<'a> {
    input_path: &'a Path,
    temporary_directory: &'a Path,
    compressor: &'a dyn Compressor,
    progress: ProgressSink,
    controller: PipelineController,
}

fn execute_slice(
    range: SliceRange,
    runtime: &SliceRuntime<'_>,
) -> Result<TemporarySlice, ZipError> {
    let total_bytes = range.length as usize;
    let progress_step = (total_bytes / 10).max(1);
    let processed = Arc::new(AtomicUsize::new(0));
    let next_log_at = Arc::new(AtomicUsize::new(progress_step));
    let logged_chunk_size = Arc::new(AtomicUsize::new(runtime.controller.chunk_size()));
    let progress_processed = Arc::clone(&processed);
    let progress_threshold = Arc::clone(&next_log_at);
    let progress_chunk_size = Arc::clone(&logged_chunk_size);
    let progress_controller = runtime.controller.clone();
    let progress_sender = runtime.progress.clone();

    runtime.progress.report(ProgressEvent::SplitProgress {
        stripe_id: range.id,
        stage: SplitStage::Running,
        bytes_processed: 0,
        bytes_written: 0,
        total_bytes,
    });
    let mut source = File::open(runtime.input_path)?;
    source.seek(SeekFrom::Start(range.start))?;
    let mut bounded_source = source.take(range.length);
    let mut temporary = tempfile_in(runtime.temporary_directory)?;
    let compressed_bytes = runtime.compressor.compress_reader_to_writer(
        &mut bounded_source,
        &mut temporary,
        &move |input_bytes, output_bytes, duration| {
            let current =
                progress_processed.fetch_add(input_bytes, Ordering::Relaxed) + input_bytes;
            let current_chunk_size = progress_controller.chunk_size();
            progress_chunk_size.store(current_chunk_size, Ordering::Relaxed);
            let threshold = progress_threshold.load(Ordering::Relaxed);
            if current >= threshold {
                let next = threshold
                    .saturating_add(progress_step)
                    .min(total_bytes.saturating_add(1));
                let _ = progress_threshold.compare_exchange(
                    threshold,
                    next,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                );
            }
            progress_sender.report(ProgressEvent::SplitProgress {
                stripe_id: range.id,
                stage: SplitStage::Running,
                bytes_processed: current.min(total_bytes),
                bytes_written: output_bytes,
                total_bytes,
            });
            progress_sender.report(ProgressEvent::AvgCompressionTime(duration));
        },
        &runtime.controller,
    )?;
    temporary.flush()?;
    runtime.progress.report(ProgressEvent::SplitProgress {
        stripe_id: range.id,
        stage: SplitStage::Done,
        bytes_processed: total_bytes,
        bytes_written: compressed_bytes,
        total_bytes,
    });
    Ok((range.id, temporary, compressed_bytes))
}

fn concatenate_slices(
    output_path: &Path,
    mut slices: Vec<TemporarySlice>,
    progress: &ProgressSink,
) -> Result<(), ZipError> {
    slices.sort_by_key(|(id, _, _)| *id);
    let mut output = File::create(output_path)?;
    let mut final_bytes_written = 0_usize;
    let mut copy_buffer = [0_u8; FINAL_COPY_BUFFER_SIZE];
    for (_, temporary, expected_bytes) in slices {
        let mut section = temporary;
        section.seek(SeekFrom::Start(0))?;
        let mut section_written = 0_usize;
        loop {
            let bytes_read = section.read(&mut copy_buffer)?;
            if bytes_read == 0 {
                break;
            }
            output.write_all(&copy_buffer[..bytes_read])?;
            section_written = section_written.saturating_add(bytes_read);
            final_bytes_written = final_bytes_written.saturating_add(bytes_read);
            progress.report(ProgressEvent::SplitFinalWrite {
                bytes_written: final_bytes_written,
            });
        }
        debug_assert_eq!(section_written, expected_bytes);
    }
    output.flush()?;
    Ok(())
}

/// Compresses fixed file ranges concurrently with bounded memory.
///
/// Each worker opens and seeks the source independently, streams its range into
/// an isolated temporary file, and returns only that file handle. The
/// coordinator concatenates completed compressed streams in source order.
pub fn compress_file(
    input_path: &Path,
    output_path: &Path,
    compressor: &dyn Compressor,
    num_threads: usize,
    sender: Sender<ProgressEvent>,
    controller: &PipelineController,
) -> Result<(), ZipError> {
    let progress = ProgressSink::new(sender);
    let input_len = std::fs::metadata(input_path)?.len();
    let requested_slices = if num_threads > 0 {
        num_threads
    } else {
        rayon::current_num_threads()
    }
    .max(1);
    let ranges = plan_ranges(input_len, requested_slices);

    crate::log_verbose!(
        "Input size: {} bytes. Streaming {} slices with {}KiB initial buffers",
        input_len,
        ranges.len(),
        controller.chunk_size() / 1024
    );

    for range in &ranges {
        progress.report(ProgressEvent::SplitProgress {
            stripe_id: range.id,
            stage: SplitStage::Waiting,
            bytes_processed: 0,
            bytes_written: 0,
            total_bytes: range.length as usize,
        });
    }

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(requested_slices)
        .build()
        .map_err(|error| ZipError::Compression(error.to_string()))?;
    let temporary_directory = output_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let runtime = SliceRuntime {
        input_path,
        temporary_directory: &temporary_directory,
        compressor,
        progress: progress.clone(),
        controller: controller.clone(),
    };
    let temporary_slices: Result<Vec<TemporarySlice>, ZipError> = pool.install(|| {
        ranges
            .into_par_iter()
            .map(|range| execute_slice(range, &runtime))
            .collect()
    });
    concatenate_slices(output_path, temporary_slices?, &progress)?;

    crate::log_verbose!("Compression completed successfully with bounded memory.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compressor::GzipCompressor;
    use std::io::Read;
    use tempfile::tempdir;

    #[test]
    fn test_split_mode_compression_streams_ranges_and_final_writes() {
        let dir = tempdir().unwrap();
        let input_path = dir.path().join("input.txt");
        let output_path = dir.path().join("output.txt.gz");
        let original_content = b"This is some sample text repeating. ".repeat(10_000);
        std::fs::write(&input_path, &original_content).unwrap();

        let compressor = GzipCompressor { level: 6 };
        let (tx, rx) = std::sync::mpsc::channel();
        let controller = PipelineController::new(6);
        compress_file(&input_path, &output_path, &compressor, 4, tx, &controller).unwrap();

        let compressed_content = std::fs::read(&output_path).unwrap();
        let mut decoder = flate2::read::MultiGzDecoder::new(&compressed_content[..]);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed).unwrap();
        assert_eq!(original_content, decompressed);

        let events = rx.try_iter().collect::<Vec<_>>();
        assert!(events.iter().any(|event| matches!(
            event,
            ProgressEvent::SplitProgress {
                stage: SplitStage::Running,
                bytes_written,
                ..
            } if *bytes_written > 0
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            ProgressEvent::SplitFinalWrite { bytes_written }
                if *bytes_written == compressed_content.len()
        )));
    }
}
