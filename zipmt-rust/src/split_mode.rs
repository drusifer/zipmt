use crate::compressor::{Compressor, ZipError};
use crate::pipeline::{PipelineController, ProgressEvent, SplitStage};
use rayon::prelude::*;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use tempfile::NamedTempFile;

const FINAL_COPY_BUFFER_SIZE: usize = 64 * 1024;

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
    let input_len = std::fs::metadata(input_path)?.len();
    let requested_slices = if num_threads > 0 {
        num_threads
    } else {
        rayon::current_num_threads()
    }
    .max(1);
    let slice_size = if input_len == 0 {
        0
    } else {
        input_len.div_ceil(requested_slices as u64)
    };
    let ranges = if input_len == 0 {
        vec![(0_usize, 0_u64, 0_u64)]
    } else {
        (0..requested_slices)
            .map(|id| {
                let start = id as u64 * slice_size;
                (id, start, slice_size.min(input_len.saturating_sub(start)))
            })
            .take_while(|(_, start, _)| *start < input_len)
            .collect::<Vec<_>>()
    };

    crate::log_verbose!(
        "Input size: {} bytes. Streaming {} slices of at most {} bytes with {}KiB initial buffers",
        input_len,
        ranges.len(),
        slice_size,
        controller.chunk_size.load(Ordering::Relaxed) / 1024
    );

    for (id, _, length) in &ranges {
        let _ = sender.send(ProgressEvent::SplitProgress {
            stripe_id: *id,
            stage: SplitStage::Waiting,
            bytes_processed: 0,
            bytes_written: 0,
            total_bytes: *length as usize,
        });
    }

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(requested_slices)
        .build()
        .map_err(|error| ZipError::Compression(error.to_string()))?;
    let input_path = input_path.to_path_buf();
    let temporary_directory = output_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let sender_clone = sender.clone();
    let controller_clone = controller.clone();

    let temporary_slices: Result<Vec<(usize, NamedTempFile, usize)>, ZipError> =
        pool.install(|| {
            ranges
                .into_par_iter()
                .map(|(id, start, length)| {
                    let worker_started = std::time::Instant::now();
                    let tx = sender_clone.clone();
                    let tx_progress = tx.clone();
                    let ctrl = controller_clone.clone();
                    let ctrl_progress = ctrl.clone();
                    let processed = Arc::new(AtomicUsize::new(0));
                    let processed_callback = processed.clone();
                    let total_bytes = length as usize;
                    let progress_step = (total_bytes / 10).max(1);
                    let next_log_at = Arc::new(AtomicUsize::new(progress_step));
                    let next_log_callback = next_log_at.clone();
                    let initial_chunk_size = ctrl.chunk_size.load(Ordering::Acquire);
                    let logged_chunk_size = Arc::new(AtomicUsize::new(initial_chunk_size));
                    let logged_chunk_callback = logged_chunk_size.clone();

                    crate::log_verbose!(
                        "Slice worker {} starting: range={}..{} ({} bytes), chunk={}KiB",
                        id,
                        start,
                        start.saturating_add(length),
                        length,
                        initial_chunk_size / 1024
                    );

                    let _ = tx.send(ProgressEvent::SplitProgress {
                        stripe_id: id,
                        stage: SplitStage::Running,
                        bytes_processed: 0,
                        bytes_written: 0,
                        total_bytes,
                    });

                    let mut source = File::open(&input_path)?;
                    source.seek(SeekFrom::Start(start))?;
                    let mut bounded_source = source.take(length);
                    let mut temporary = NamedTempFile::new_in(&temporary_directory)?;
                    crate::log_verbose!(
                        "Slice worker {} streaming encoder output to {:?}",
                        id,
                        temporary.path()
                    );
                    let compressed_bytes = compressor.compress_reader_to_writer(
                        &mut bounded_source,
                        temporary.as_file_mut(),
                        &move |input_bytes, output_bytes, duration| {
                            let current = processed_callback
                                .fetch_add(input_bytes, Ordering::Relaxed)
                                + input_bytes;
                            let current_chunk_size =
                                ctrl_progress.chunk_size.load(Ordering::Acquire);
                            let previous_chunk_size =
                                logged_chunk_callback.swap(current_chunk_size, Ordering::Relaxed);
                            if current_chunk_size != previous_chunk_size {
                                crate::log_verbose!(
                                    "Slice worker {} replaced read chunk: {}KiB -> {}KiB",
                                    id,
                                    previous_chunk_size / 1024,
                                    current_chunk_size / 1024
                                );
                            }
                            let threshold = next_log_callback.load(Ordering::Relaxed);
                            if current >= threshold {
                                let next = threshold
                                    .saturating_add(progress_step)
                                    .min(total_bytes.saturating_add(1));
                                if next_log_callback
                                    .compare_exchange(
                                        threshold,
                                        next,
                                        Ordering::Relaxed,
                                        Ordering::Relaxed,
                                    )
                                    .is_ok()
                                {
                                    crate::log_verbose!(
                                        "Slice worker {} progress: {}/{} bytes ({:.1}%), compressed={} bytes",
                                        id,
                                        current.min(total_bytes),
                                        total_bytes,
                                        if total_bytes == 0 {
                                            100.0
                                        } else {
                                            current.min(total_bytes) as f64 * 100.0
                                                / total_bytes as f64
                                        },
                                        output_bytes
                                    );
                                }
                            }
                            let _ = tx_progress.send(ProgressEvent::SplitProgress {
                                stripe_id: id,
                                stage: SplitStage::Running,
                                bytes_processed: current.min(total_bytes),
                                bytes_written: output_bytes,
                                total_bytes,
                            });
                            let _ = tx_progress.send(ProgressEvent::AvgCompressionTime(duration));
                        },
                        &ctrl,
                    )?;
                    temporary.as_file_mut().flush()?;
                    crate::log_verbose!(
                        "Slice worker {} complete: {} -> {} bytes in {:.2}s",
                        id,
                        total_bytes,
                        compressed_bytes,
                        worker_started.elapsed().as_secs_f64()
                    );
                    let _ = tx.send(ProgressEvent::SplitProgress {
                        stripe_id: id,
                        stage: SplitStage::Done,
                        bytes_processed: total_bytes,
                        bytes_written: compressed_bytes,
                        total_bytes,
                    });
                    Ok((id, temporary, compressed_bytes))
                })
                .collect()
        });

    let mut temporary_slices = temporary_slices?;
    temporary_slices.sort_by_key(|(id, _, _)| *id);
    crate::log_verbose!(
        "Concatenating {} temporary compressed slices into {:?}",
        temporary_slices.len(),
        output_path
    );

    let mut output = File::create(output_path)?;
    let mut final_bytes_written = 0_usize;
    let mut copy_buffer = [0_u8; FINAL_COPY_BUFFER_SIZE];
    for (id, temporary, expected_bytes) in temporary_slices {
        crate::log_verbose!(
            "Final output: appending slice {} ({} bytes) from {:?}",
            id,
            expected_bytes,
            temporary.path()
        );
        let mut section = temporary.reopen()?;
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
            let _ = sender.send(ProgressEvent::SplitFinalWrite {
                bytes_written: final_bytes_written,
            });
        }
        debug_assert_eq!(section_written, expected_bytes);
        crate::log_verbose!(
            "Final output: slice {} appended; total={} bytes",
            id,
            final_bytes_written
        );
    }
    output.flush()?;

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
