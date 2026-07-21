use std::time::Instant;

use crate::pipeline::{ProgressEvent, SplitStage, WorkerStage};

use super::TuiState;

/// Applies one progress event without terminal, process, or filesystem I/O.
///
/// A returned duration asks the runtime wrapper to sample process metrics after
/// the pure transition has completed.
pub(super) fn reduce_progress_event(
    state: &mut TuiState,
    event: ProgressEvent,
    now: Instant,
) -> Option<std::time::Duration> {
    match event {
        ProgressEvent::SplitProgress {
            stripe_id,
            stage,
            bytes_processed,
            bytes_written,
            total_bytes,
        } => reduce_split_progress(
            state,
            stripe_id,
            stage,
            bytes_processed,
            bytes_written,
            total_bytes,
            now,
        ),
        ProgressEvent::SplitFinalWrite { bytes_written } => {
            state.split_final_bytes_written = state.split_final_bytes_written.max(bytes_written);
        }
        ProgressEvent::StreamProgress {
            bytes_read,
            bytes_written,
            queue_depth,
        } => reduce_stream_progress(state, bytes_read, bytes_written, queue_depth),
        ProgressEvent::WorkerStatus {
            worker_id,
            stage,
            current_chunk,
        } => reduce_worker_status(state, worker_id, stage, current_chunk),
        ProgressEvent::WorkerChunkProgress {
            worker_id,
            seq_num,
            bytes_processed,
            bytes_written,
            total_bytes,
            finalized,
        } => reduce_worker_progress(
            state,
            worker_id,
            seq_num,
            bytes_processed,
            bytes_written,
            total_bytes,
            finalized,
            now,
        ),
        ProgressEvent::ChunkQueued { seq_num, .. } => reduce_chunk_queued(state, seq_num),
        ProgressEvent::ChunkAssigned { worker_id, seq_num } => {
            reduce_chunk_assigned(state, worker_id, seq_num, now);
        }
        ProgressEvent::ChunkPending { worker_id, seq_num } => {
            reduce_chunk_pending(state, worker_id, seq_num);
        }
        ProgressEvent::ChunkWritten { seq_num } => reduce_chunk_written(state, seq_num),
        ProgressEvent::WorkerAvailability { worker_id, enabled } => {
            reduce_worker_availability(state, worker_id, enabled);
        }
        ProgressEvent::AvgCompressionTime(duration) => state.update_chunk_time(duration),
        ProgressEvent::Error(_) => clear_transient_pipeline_state(state),
        ProgressEvent::Complete => {
            let system_elapsed = now.duration_since(state.last_speed_update);
            state.sample_io_bucket(now);
            state.is_complete = true;
            state.final_elapsed = Some(now.saturating_duration_since(state.start_time));
            clear_transient_pipeline_state(state);
            return (!system_elapsed.is_zero()).then_some(system_elapsed);
        }
    }
    None
}

fn reduce_split_progress(
    state: &mut TuiState,
    stripe_id: usize,
    stage: SplitStage,
    bytes_processed: usize,
    bytes_written: usize,
    total_bytes: usize,
    now: Instant,
) {
    if let Some(stripe) = state.stripes.get_mut(stripe_id) {
        stripe.stage = stage;
        if stage == SplitStage::Running && stripe.started_at.is_none() {
            stripe.started_at = Some(now);
        }
        if stage == SplitStage::Done && stripe.completed_at.is_none() {
            stripe.completed_at = Some(now);
        }
        stripe.bytes_processed = bytes_processed.min(total_bytes);
        stripe.bytes_written = stripe.bytes_written.max(bytes_written);
        stripe.total_bytes = total_bytes;
    }
}

fn reduce_stream_progress(
    state: &mut TuiState,
    bytes_read: usize,
    bytes_written: usize,
    queue_depth: usize,
) {
    state.bytes_read = bytes_read;
    state.bytes_written = bytes_written;
    state.queue_depth = queue_depth;
}

fn reduce_worker_status(
    state: &mut TuiState,
    worker_id: usize,
    stage: WorkerStage,
    current_chunk: Option<u64>,
) {
    if let Some(worker) = state.workers.get_mut(worker_id) {
        worker.stage = stage;
        worker.current_chunk = current_chunk;
    }
}

#[allow(clippy::too_many_arguments)]
fn reduce_worker_progress(
    state: &mut TuiState,
    worker_id: usize,
    seq_num: u64,
    bytes_processed: usize,
    bytes_written: usize,
    total_bytes: usize,
    finalized: bool,
    now: Instant,
) {
    if let Some(worker) = state.workers.get_mut(worker_id) {
        worker.display_chunk = Some(seq_num);
        worker.bytes_processed = bytes_processed.min(total_bytes);
        worker.total_bytes = total_bytes;
        worker.bytes_written = worker.bytes_written.max(bytes_written);
        if worker.started_at.is_none() {
            worker.started_at = Some(now);
        }
        if finalized && worker.completed_at.is_none() {
            worker.completed_at = Some(now);
            worker.record_final_metrics(now);
        }
    }
}

fn reduce_chunk_queued(state: &mut TuiState, seq_num: u64) {
    if !state.input_queue.contains(&seq_num) {
        state.input_queue.push(seq_num);
    }
}

fn reduce_chunk_assigned(state: &mut TuiState, worker_id: usize, seq_num: u64, now: Instant) {
    state.input_queue.retain(|queued| *queued != seq_num);
    if let Some(worker) = state.workers.get_mut(worker_id) {
        worker.stage = WorkerStage::Busy;
        worker.current_chunk = Some(seq_num);
        worker.display_chunk = Some(seq_num);
        worker.bytes_processed = 0;
        worker.total_bytes = 0;
        worker.bytes_written = 0;
        worker.started_at = Some(now);
        worker.completed_at = None;
    }
}

fn reduce_chunk_pending(state: &mut TuiState, worker_id: usize, seq_num: u64) {
    if let Some(worker) = state.workers.get_mut(worker_id)
        && worker.current_chunk == Some(seq_num)
    {
        worker.stage = WorkerStage::Idle;
        worker.current_chunk = None;
    }
    if !state.output_buffer.contains(&seq_num) {
        state.output_buffer.push(seq_num);
        state.output_buffer.sort_unstable();
    }
}

fn reduce_chunk_written(state: &mut TuiState, seq_num: u64) {
    state.output_buffer.retain(|pending| *pending != seq_num);
    state.next_expected_seq = state.next_expected_seq.max(seq_num + 1);
}

fn reduce_worker_availability(state: &mut TuiState, worker_id: usize, enabled: bool) {
    if let Some(worker) = state.workers.get_mut(worker_id) {
        worker.stage = if enabled {
            WorkerStage::Idle
        } else {
            WorkerStage::Off
        };
        if !enabled {
            worker.current_chunk = None;
        }
    }
}

fn clear_transient_pipeline_state(state: &mut TuiState) {
    state.input_queue.clear();
    state.output_buffer.clear();
    state.queue_depth = 0;
    for worker in &mut state.workers {
        worker.current_chunk = None;
        if worker.stage != WorkerStage::Off {
            worker.stage = WorkerStage::Idle;
        }
    }
}
