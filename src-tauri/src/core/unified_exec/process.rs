//! UnifiedExecProcess — wraps a PTY/pipe process with output buffering and exit detection.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::{Mutex, Notify};
use tokio::sync::oneshot::error::TryRecvError;
use tokio::task::JoinHandle;
use tokio::time::Duration;
use tokio_util::sync::CancellationToken;

use crate::pty::ProcessHandle;
use super::head_tail_buffer::HeadTailBuffer;

pub type OutputBuffer = Arc<Mutex<HeadTailBuffer>>;

/// Cloneable handles for polling output from outside the process.
pub struct OutputHandles {
    pub output_buffer: OutputBuffer,
    pub output_notify: Arc<Notify>,
    pub output_closed: Arc<AtomicBool>,
    pub output_closed_notify: Arc<Notify>,
    pub cancellation_token: CancellationToken,
}

pub struct UnifiedExecProcess {
    process_handle: ProcessHandle,
    output_buffer: OutputBuffer,
    output_notify: Arc<Notify>,
    output_closed: Arc<AtomicBool>,
    output_closed_notify: Arc<Notify>,
    cancellation_token: CancellationToken,
    output_drained: Arc<Notify>,
    output_task: JoinHandle<()>,
}

impl UnifiedExecProcess {
    pub fn new(
        process_handle: ProcessHandle,
        initial_output_rx: tokio::sync::broadcast::Receiver<Vec<u8>>,
    ) -> Self {
        let output_buffer = Arc::new(Mutex::new(HeadTailBuffer::default()));
        let output_notify = Arc::new(Notify::new());
        let output_closed = Arc::new(AtomicBool::new(false));
        let output_closed_notify = Arc::new(Notify::new());
        let cancellation_token = CancellationToken::new();
        let output_drained = Arc::new(Notify::new());

        let mut receiver = initial_output_rx;
        let buf_clone = Arc::clone(&output_buffer);
        let notify_clone = Arc::clone(&output_notify);
        let closed_clone = Arc::clone(&output_closed);
        let closed_notify_clone = Arc::clone(&output_closed_notify);

        let output_task = tokio::spawn(async move {
            loop {
                match receiver.recv().await {
                    Ok(chunk) => {
                        buf_clone.lock().await.push_chunk(chunk);
                        notify_clone.notify_waiters();
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        closed_clone.store(true, Ordering::Release);
                        closed_notify_clone.notify_waiters();
                        break;
                    }
                }
            }
        });

        Self {
            process_handle,
            output_buffer,
            output_notify,
            output_closed,
            output_closed_notify,
            cancellation_token,
            output_drained,
            output_task,
        }
    }

    pub fn writer_sender(&self) -> tokio::sync::mpsc::Sender<Vec<u8>> {
        self.process_handle.writer_sender()
    }

    pub fn output_receiver(&self) -> tokio::sync::broadcast::Receiver<Vec<u8>> {
        self.process_handle.output_receiver()
    }

    pub fn output_handles(&self) -> OutputHandles {
        OutputHandles {
            output_buffer: Arc::clone(&self.output_buffer),
            output_notify: Arc::clone(&self.output_notify),
            output_closed: Arc::clone(&self.output_closed),
            output_closed_notify: Arc::clone(&self.output_closed_notify),
            cancellation_token: self.cancellation_token.clone(),
        }
    }

    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation_token.clone()
    }

    pub fn output_drained_notify(&self) -> Arc<Notify> {
        Arc::clone(&self.output_drained)
    }

    pub fn has_exited(&self) -> bool {
        self.process_handle.has_exited()
    }

    pub fn exit_code(&self) -> Option<i32> {
        self.process_handle.exit_code()
    }

    pub fn terminate(&self) {
        self.output_closed.store(true, Ordering::Release);
        self.output_closed_notify.notify_waiters();
        self.process_handle.terminate();
        self.cancellation_token.cancel();
        self.output_task.abort();
    }

    pub async fn snapshot_output(&self) -> Vec<Vec<u8>> {
        self.output_buffer.lock().await.snapshot_chunks()
    }

    /// Create from a SpawnedProcess. Waits briefly for early exit (sandbox denial detection).
    pub async fn from_spawned(
        spawned: crate::pty::SpawnedProcess,
    ) -> Result<Self, super::UnifiedExecError> {
        let crate::pty::SpawnedProcess {
            session: process_handle,
            output_rx,
            mut exit_rx,
        } = spawned;
        let managed = Self::new(process_handle, output_rx);

        // Check if already exited (e.g. sandbox denial).
        let exit_ready = matches!(exit_rx.try_recv(), Ok(_) | Err(TryRecvError::Closed));
        if exit_ready {
            managed.cancellation_token.cancel();
            return Ok(managed);
        }

        // Brief wait for early exit.
        if tokio::time::timeout(Duration::from_millis(150), &mut exit_rx)
            .await
            .is_ok()
        {
            managed.cancellation_token.cancel();
            return Ok(managed);
        }

        // Still running — spawn background exit watcher.
        tokio::spawn({
            let token = managed.cancellation_token.clone();
            async move {
                let _ = exit_rx.await;
                token.cancel();
            }
        });

        Ok(managed)
    }
}

impl Drop for UnifiedExecProcess {
    fn drop(&mut self) {
        self.terminate();
    }
}
