//! Async output watcher — streams process output into a transcript buffer
//! and monitors process exit.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};

use super::head_tail_buffer::HeadTailBuffer;
use super::process::UnifiedExecProcess;

/// Upper bound for a single output delta chunk (8 KiB).
pub const OUTPUT_DELTA_MAX_BYTES: usize = 8192;

/// Grace period after exit to drain remaining output.
pub const TRAILING_OUTPUT_GRACE: Duration = Duration::from_millis(100);

/// Spawn a background task that reads PTY output into a shared transcript buffer.
pub fn start_streaming_output(
    process: &UnifiedExecProcess,
    transcript: Arc<Mutex<HeadTailBuffer>>,
) {
    let mut receiver = process.output_receiver();
    let output_drained = process.output_drained_notify();
    let exit_token = process.cancellation_token();

    tokio::spawn(async move {
        use std::pin::Pin;
        use tokio::sync::broadcast::error::RecvError;
        use tokio::time::Sleep;

        let mut pending = Vec::<u8>::new();
        let mut grace_sleep: Option<Pin<Box<Sleep>>> = None;

        loop {
            tokio::select! {
                _ = exit_token.cancelled(), if grace_sleep.is_none() => {
                    let deadline = Instant::now() + TRAILING_OUTPUT_GRACE;
                    grace_sleep.replace(Box::pin(tokio::time::sleep_until(deadline)));
                }

                _ = async {
                    if let Some(sleep) = grace_sleep.as_mut() {
                        sleep.as_mut().await;
                    }
                }, if grace_sleep.is_some() => {
                    // Flush remaining pending bytes
                    if !pending.is_empty() {
                        let mut guard = transcript.lock().await;
                        guard.push_chunk(std::mem::take(&mut pending));
                    }
                    output_drained.notify_one();
                    break;
                }

                received = receiver.recv() => {
                    match received {
                        Ok(chunk) => {
                            pending.extend_from_slice(&chunk);
                            // Flush complete UTF-8 prefixes into transcript
                            while let Some(prefix) = split_valid_utf8_prefix(&mut pending, OUTPUT_DELTA_MAX_BYTES) {
                                transcript.lock().await.push_chunk(prefix);
                            }
                        }
                        Err(RecvError::Lagged(_)) => continue,
                        Err(RecvError::Closed) => {
                            if !pending.is_empty() {
                                transcript.lock().await.push_chunk(std::mem::take(&mut pending));
                            }
                            output_drained.notify_one();
                            break;
                        }
                    }
                }
            }
        }
    });
}

/// Spawn a background watcher that waits for process exit.
#[allow(clippy::too_many_arguments)]
pub fn spawn_exit_watcher(
    process: Arc<UnifiedExecProcess>,
    _call_id: String,
    _command: Vec<String>,
    _cwd: PathBuf,
    _process_id: String,
    _transcript: Arc<Mutex<HeadTailBuffer>>,
    _started_at: Instant,
) {
    let exit_token = process.cancellation_token();
    let output_drained = process.output_drained_notify();

    tokio::spawn(async move {
        exit_token.cancelled().await;
        output_drained.notified().await;
        // Process has exited and output is drained.
        // In a full implementation this would emit an ExecCommandEnd event.
        // The process will be cleaned up on next write_stdin or refresh_process_state.
    });
}

/// Resolve aggregated output from transcript, falling back to provided text.
pub async fn resolve_aggregated_output(
    transcript: &Arc<Mutex<HeadTailBuffer>>,
    fallback: String,
) -> String {
    let guard = transcript.lock().await;
    if guard.retained_bytes() == 0 {
        return fallback;
    }
    String::from_utf8_lossy(&guard.to_bytes()).to_string()
}

/// Split the longest valid UTF-8 prefix from `buffer` (up to `max_bytes`),
/// draining those bytes. Returns `None` if buffer is empty.
pub fn split_valid_utf8_prefix(buffer: &mut Vec<u8>, max_bytes: usize) -> Option<Vec<u8>> {
    if buffer.is_empty() {
        return None;
    }
    let max_len = buffer.len().min(max_bytes);
    let mut split = max_len;
    while split > 0 {
        if std::str::from_utf8(&buffer[..split]).is_ok() {
            let prefix = buffer[..split].to_vec();
            buffer.drain(..split);
            return Some(prefix);
        }
        if max_len - split > 4 {
            break;
        }
        split -= 1;
    }
    // Emit single byte to keep progress.
    let byte = buffer.drain(..1).collect();
    Some(byte)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_respects_max_bytes_for_ascii() {
        let mut buf = b"hello word!".to_vec();
        let first = split_valid_utf8_prefix(&mut buf, 5).unwrap();
        assert_eq!(first, b"hello");
        assert_eq!(buf, b" word!");
    }

    #[test]
    fn split_avoids_splitting_utf8_codepoints() {
        let mut buf = "ééé".as_bytes().to_vec();
        let first = split_valid_utf8_prefix(&mut buf, 3).unwrap();
        assert_eq!(std::str::from_utf8(&first).unwrap(), "é");
    }

    #[test]
    fn split_makes_progress_on_invalid_utf8() {
        let mut buf = vec![0xff, b'a', b'b'];
        let first = split_valid_utf8_prefix(&mut buf, 2).unwrap();
        assert_eq!(first, vec![0xff]);
        assert_eq!(buf, b"ab");
    }

    #[test]
    fn split_empty_returns_none() {
        let mut buf = Vec::new();
        assert!(split_valid_utf8_prefix(&mut buf, 10).is_none());
    }
}
