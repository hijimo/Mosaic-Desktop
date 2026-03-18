//! Process lifecycle management — PTY-backed exec_command and write_stdin.

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use tokio::sync::{mpsc, Notify};
use tokio::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

use crate::core::truncation::{approx_token_count, formatted_truncate_text, TruncationPolicy};
use crate::pty;

use super::async_watcher::{spawn_exit_watcher, start_streaming_output};
use super::head_tail_buffer::HeadTailBuffer;
use super::process::{OutputBuffer, OutputHandles, UnifiedExecProcess};
use super::{
    apply_exec_env, clamp_yield_time, generate_chunk_id, resolve_max_tokens,
    ExecCommandRequest, ProcessEntry, ProcessStore, UnifiedExecError, UnifiedExecProcessManager,
    UnifiedExecResponse, WriteStdinRequest,
    MAX_PROCESSES, MAX_YIELD_TIME_MS, MIN_EMPTY_YIELD_TIME_MS, MIN_YIELD_TIME_MS,
    OUTPUT_MAX_TOKENS, WARNING_PROCESSES,
};

impl UnifiedExecProcessManager {
    /// Reserve a unique numeric process ID.
    pub async fn allocate_process_id(&self) -> String {
        loop {
            let mut store = self.process_store.lock().await;
            let process_id = rand::Rng::random_range(&mut rand::rng(), 1_000..100_000).to_string();
            if !store.reserved_process_ids.contains(&process_id) {
                store.reserved_process_ids.insert(process_id.clone());
                return process_id;
            }
        }
    }

    /// Release a process ID and remove the process from the store.
    pub async fn release_process_id(&self, process_id: &str) {
        let removed = {
            let mut store = self.process_store.lock().await;
            store.remove(process_id)
        };
        if let Some(entry) = removed {
            entry.process.terminate();
        }
    }

    /// Spawn a PTY (or pipe) process from a command + env.
    pub async fn open_session_with_exec_env(
        &self,
        command: &[String],
        cwd: &PathBuf,
        env: &HashMap<String, String>,
        tty: bool,
    ) -> Result<UnifiedExecProcess, UnifiedExecError> {
        let (program, args) = command
            .split_first()
            .ok_or(UnifiedExecError::MissingCommandLine)?;

        let spawn_result = if tty {
            pty::pty::spawn_process(program, args, cwd.as_path(), env, &None).await
        } else {
            pty::pipe::spawn_process_no_stdin(program, args, cwd.as_path(), env, &None).await
        };

        let spawned =
            spawn_result.map_err(|err| UnifiedExecError::create_process(err.to_string()))?;
        UnifiedExecProcess::from_spawned(spawned).await
    }

    /// Execute a command, returning output after yield_time_ms or process exit.
    pub async fn exec_command(
        &self,
        request: ExecCommandRequest,
        call_id: &str,
    ) -> Result<UnifiedExecResponse, UnifiedExecError> {
        let cwd = request
            .workdir
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

        // Build env
        let env = apply_exec_env(HashMap::new());

        // Spawn PTY process
        let process = self
            .open_session_with_exec_env(&request.command, &cwd, &env, request.tty)
            .await;

        let process = match process {
            Ok(p) => Arc::new(p),
            Err(err) => {
                self.release_process_id(&request.process_id).await;
                return Err(err);
            }
        };

        // Set up streaming transcript
        let transcript = Arc::new(tokio::sync::Mutex::new(HeadTailBuffer::default()));
        start_streaming_output(&process, Arc::clone(&transcript));

        let max_tokens = resolve_max_tokens(request.max_output_tokens);
        let yield_time_ms = clamp_yield_time(request.yield_time_ms);

        let start = Instant::now();
        let OutputHandles {
            output_buffer,
            output_notify,
            output_closed,
            output_closed_notify,
            cancellation_token,
        } = process.output_handles();

        let deadline = start + Duration::from_millis(yield_time_ms);
        let collected = Self::collect_output_until_deadline(
            &output_buffer,
            &output_notify,
            &output_closed,
            &output_closed_notify,
            &cancellation_token,
            deadline,
        )
        .await;
        let wall_time = Instant::now().saturating_duration_since(start);

        let text = String::from_utf8_lossy(&collected).to_string();
        let output = formatted_truncate_text(
            &text,
            TruncationPolicy::KeepRecentTokens { max_tokens },
        );
        let exit_code = process.exit_code();
        let has_exited = process.has_exited() || exit_code.is_some();
        let chunk_id = generate_chunk_id();
        let original_token_count = approx_token_count(&text);

        if has_exited {
            self.release_process_id(&request.process_id).await;
        } else {
            // Long-lived: store for write_stdin reuse
            self.store_process(
                Arc::clone(&process),
                call_id,
                &request.command,
                cwd.clone(),
                start,
                request.process_id.clone(),
                request.tty,
                Arc::clone(&transcript),
            )
            .await;
        }

        Ok(UnifiedExecResponse {
            event_call_id: call_id.to_string(),
            chunk_id,
            wall_time,
            output,
            raw_output: collected,
            process_id: if has_exited {
                None
            } else {
                Some(request.process_id.clone())
            },
            exit_code,
            original_token_count: Some(original_token_count),
            session_command: Some(request.command.clone()),
        })
    }

    /// Write to stdin of an existing process and collect output.
    pub async fn write_stdin(
        &self,
        request: WriteStdinRequest<'_>,
    ) -> Result<UnifiedExecResponse, UnifiedExecError> {
        let prepared = self.prepare_process_handles(request.process_id).await?;

        if !request.input.is_empty() {
            if !prepared.tty {
                return Err(UnifiedExecError::StdinClosed);
            }
            Self::send_input(&prepared.writer_tx, request.input.as_bytes()).await?;
            // Brief pause so the process can react
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        let max_tokens = resolve_max_tokens(request.max_output_tokens);
        let yield_time_ms = {
            let time_ms = request.yield_time_ms.max(MIN_YIELD_TIME_MS);
            if request.input.is_empty() {
                time_ms.clamp(MIN_EMPTY_YIELD_TIME_MS, self.max_write_stdin_yield_time_ms)
            } else {
                time_ms.min(MAX_YIELD_TIME_MS)
            }
        };

        let start = Instant::now();
        let deadline = start + Duration::from_millis(yield_time_ms);
        let collected = Self::collect_output_until_deadline(
            &prepared.output_buffer,
            &prepared.output_notify,
            &prepared.output_closed,
            &prepared.output_closed_notify,
            &prepared.cancellation_token,
            deadline,
        )
        .await;
        let wall_time = Instant::now().saturating_duration_since(start);

        let text = String::from_utf8_lossy(&collected).to_string();
        let output = formatted_truncate_text(
            &text,
            TruncationPolicy::KeepRecentTokens { max_tokens },
        );
        let original_token_count = approx_token_count(&text);
        let chunk_id = generate_chunk_id();

        // Check if process has exited
        let status = self.refresh_process_state(&prepared.process_id).await;
        let (process_id, exit_code, event_call_id) = match status {
            ProcessStatus::Alive { exit_code, call_id, process_id } => {
                (Some(process_id), exit_code, call_id)
            }
            ProcessStatus::Exited { exit_code, call_id } => {
                (None, exit_code, call_id)
            }
            ProcessStatus::Unknown => {
                return Err(UnifiedExecError::UnknownProcessId {
                    process_id: request.process_id.to_string(),
                });
            }
        };

        Ok(UnifiedExecResponse {
            event_call_id,
            chunk_id,
            wall_time,
            output,
            raw_output: collected,
            process_id,
            exit_code,
            original_token_count: Some(original_token_count),
            session_command: Some(prepared.command),
        })
    }

    /// Collect output from the buffer until deadline, handling exit gracefully.
    pub(crate) async fn collect_output_until_deadline(
        output_buffer: &OutputBuffer,
        output_notify: &Arc<Notify>,
        output_closed: &Arc<std::sync::atomic::AtomicBool>,
        output_closed_notify: &Arc<Notify>,
        cancellation_token: &CancellationToken,
        deadline: Instant,
    ) -> Vec<u8> {
        const POST_EXIT_CLOSE_WAIT_CAP: Duration = Duration::from_millis(50);

        let mut collected: Vec<u8> = Vec::with_capacity(4096);
        let mut exit_signal_received = cancellation_token.is_cancelled();
        let mut post_exit_deadline: Option<Instant> = None;

        loop {
            let drained_chunks: Vec<Vec<u8>>;
            let mut wait_for_output = None;
            {
                let mut guard = output_buffer.lock().await;
                drained_chunks = guard.drain_chunks();
                if drained_chunks.is_empty() {
                    wait_for_output = Some(output_notify.notified());
                }
            }

            if drained_chunks.is_empty() {
                exit_signal_received |= cancellation_token.is_cancelled();
                if exit_signal_received && output_closed.load(Ordering::Acquire) {
                    break;
                }
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining == Duration::ZERO {
                    break;
                }

                if exit_signal_received {
                    let now = Instant::now();
                    let close_wait_deadline = *post_exit_deadline
                        .get_or_insert_with(|| now + remaining.min(POST_EXIT_CLOSE_WAIT_CAP));
                    let close_wait_remaining = close_wait_deadline.saturating_duration_since(now);
                    if close_wait_remaining == Duration::ZERO {
                        break;
                    }
                    let notified = wait_for_output.unwrap_or_else(|| output_notify.notified());
                    let closed = output_closed_notify.notified();
                    tokio::pin!(notified);
                    tokio::pin!(closed);
                    tokio::select! {
                        _ = &mut notified => {}
                        _ = &mut closed => {}
                        _ = tokio::time::sleep(close_wait_remaining) => break,
                    }
                    continue;
                }

                let notified = wait_for_output.unwrap_or_else(|| output_notify.notified());
                tokio::pin!(notified);
                let exit_notified = cancellation_token.cancelled();
                tokio::pin!(exit_notified);
                tokio::select! {
                    _ = &mut notified => {}
                    _ = &mut exit_notified => exit_signal_received = true,
                    _ = tokio::time::sleep(remaining) => break,
                }
                continue;
            }

            for chunk in drained_chunks {
                collected.extend_from_slice(&chunk);
            }

            exit_signal_received |= cancellation_token.is_cancelled();
            if Instant::now() >= deadline {
                break;
            }
        }

        collected
    }

    /// Store a long-lived process for write_stdin reuse.
    #[allow(clippy::too_many_arguments)]
    async fn store_process(
        &self,
        process: Arc<UnifiedExecProcess>,
        call_id: &str,
        command: &[String],
        cwd: PathBuf,
        started_at: Instant,
        process_id: String,
        tty: bool,
        transcript: Arc<tokio::sync::Mutex<HeadTailBuffer>>,
    ) {
        let entry = ProcessEntry {
            process: Arc::clone(&process),
            call_id: call_id.to_string(),
            process_id: process_id.clone(),
            command: command.to_vec(),
            tty,
            last_used: started_at,
        };

        let pruned_entry = {
            let mut store = self.process_store.lock().await;
            let pruned = Self::prune_processes_if_needed(&mut store);
            store.processes.insert(process_id.clone(), entry);
            pruned
        };

        if let Some(pruned) = pruned_entry {
            pruned.process.terminate();
        }

        spawn_exit_watcher(
            process,
            call_id.to_string(),
            command.to_vec(),
            cwd,
            process_id,
            transcript,
            started_at,
        );
    }

    /// Check if process has exited and clean up if so.
    async fn refresh_process_state(&self, process_id: &str) -> ProcessStatus {
        let mut store = self.process_store.lock().await;
        let Some(entry) = store.processes.get(process_id) else {
            return ProcessStatus::Unknown;
        };

        let exit_code = entry.process.exit_code();
        let pid = entry.process_id.clone();
        let call_id = entry.call_id.clone();

        if entry.process.has_exited() {
            if let Some(removed) = store.remove(&pid) {
                removed.process.terminate();
            }
            ProcessStatus::Exited { exit_code, call_id }
        } else {
            ProcessStatus::Alive {
                exit_code,
                call_id,
                process_id: pid,
            }
        }
    }

    /// Get handles for an existing process (for write_stdin).
    async fn prepare_process_handles(
        &self,
        process_id: &str,
    ) -> Result<PreparedProcessHandles, UnifiedExecError> {
        let mut store = self.process_store.lock().await;
        let entry = store
            .processes
            .get_mut(process_id)
            .ok_or(UnifiedExecError::UnknownProcessId {
                process_id: process_id.to_string(),
            })?;
        entry.last_used = Instant::now();
        let OutputHandles {
            output_buffer,
            output_notify,
            output_closed,
            output_closed_notify,
            cancellation_token,
        } = entry.process.output_handles();

        Ok(PreparedProcessHandles {
            writer_tx: entry.process.writer_sender(),
            output_buffer,
            output_notify,
            output_closed,
            output_closed_notify,
            cancellation_token,
            command: entry.command.clone(),
            process_id: entry.process_id.clone(),
            tty: entry.tty,
        })
    }

    async fn send_input(
        writer_tx: &mpsc::Sender<Vec<u8>>,
        data: &[u8],
    ) -> Result<(), UnifiedExecError> {
        writer_tx
            .send(data.to_vec())
            .await
            .map_err(|_| UnifiedExecError::WriteToStdin)
    }

    /// Evict the least-recently-used process when at capacity.
    fn prune_processes_if_needed(store: &mut ProcessStore) -> Option<ProcessEntry> {
        if store.processes.len() < MAX_PROCESSES {
            return None;
        }

        let meta: Vec<(String, Instant, bool)> = store
            .processes
            .iter()
            .map(|(id, e)| (id.clone(), e.last_used, e.process.has_exited()))
            .collect();

        if let Some(pid) = Self::process_id_to_prune(&meta) {
            return store.remove(&pid);
        }
        None
    }

    /// Pruning policy: protect 8 most recent, prefer exited, then LRU.
    fn process_id_to_prune(meta: &[(String, Instant, bool)]) -> Option<String> {
        if meta.is_empty() {
            return None;
        }
        let mut by_recency = meta.to_vec();
        by_recency.sort_by_key(|(_, last_used, _)| Reverse(*last_used));
        let protected: HashSet<String> = by_recency
            .iter()
            .take(8)
            .map(|(id, _, _)| id.clone())
            .collect();

        let mut lru = meta.to_vec();
        lru.sort_by_key(|(_, last_used, _)| *last_used);

        // Prefer exited processes outside protected set
        if let Some((id, _, _)) = lru
            .iter()
            .find(|(id, _, exited)| !protected.contains(id) && *exited)
        {
            return Some(id.clone());
        }
        // Fall back to LRU outside protected set
        lru.into_iter()
            .find(|(id, _, _)| !protected.contains(id))
            .map(|(id, _, _)| id)
    }

    /// Terminate all managed processes.
    pub async fn terminate_all_processes(&self) {
        let entries: Vec<ProcessEntry> = {
            let mut store = self.process_store.lock().await;
            let entries: Vec<ProcessEntry> = store.processes.drain().map(|(_, e)| e).collect();
            store.reserved_process_ids.clear();
            entries
        };
        for entry in entries {
            entry.process.terminate();
        }
    }
}

struct PreparedProcessHandles {
    writer_tx: mpsc::Sender<Vec<u8>>,
    output_buffer: OutputBuffer,
    output_notify: Arc<Notify>,
    output_closed: Arc<std::sync::atomic::AtomicBool>,
    output_closed_notify: Arc<Notify>,
    cancellation_token: CancellationToken,
    command: Vec<String>,
    process_id: String,
    tty: bool,
}

enum ProcessStatus {
    Alive {
        exit_code: Option<i32>,
        call_id: String,
        process_id: String,
    },
    Exited {
        exit_code: Option<i32>,
        call_id: String,
    },
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pruning_prefers_exited_outside_protected() {
        let now = Instant::now();
        let id = |n: i32| n.to_string();
        let meta = vec![
            (id(1), now - Duration::from_secs(40), false),
            (id(2), now - Duration::from_secs(30), true),
            (id(3), now - Duration::from_secs(20), false),
            (id(4), now - Duration::from_secs(19), false),
            (id(5), now - Duration::from_secs(18), false),
            (id(6), now - Duration::from_secs(17), false),
            (id(7), now - Duration::from_secs(16), false),
            (id(8), now - Duration::from_secs(15), false),
            (id(9), now - Duration::from_secs(14), false),
            (id(10), now - Duration::from_secs(13), false),
        ];
        assert_eq!(
            UnifiedExecProcessManager::process_id_to_prune(&meta),
            Some(id(2))
        );
    }

    #[test]
    fn pruning_falls_back_to_lru() {
        let now = Instant::now();
        let id = |n: i32| n.to_string();
        let meta = vec![
            (id(1), now - Duration::from_secs(40), false),
            (id(2), now - Duration::from_secs(30), false),
            (id(3), now - Duration::from_secs(20), false),
            (id(4), now - Duration::from_secs(19), false),
            (id(5), now - Duration::from_secs(18), false),
            (id(6), now - Duration::from_secs(17), false),
            (id(7), now - Duration::from_secs(16), false),
            (id(8), now - Duration::from_secs(15), false),
            (id(9), now - Duration::from_secs(14), false),
            (id(10), now - Duration::from_secs(13), false),
        ];
        assert_eq!(
            UnifiedExecProcessManager::process_id_to_prune(&meta),
            Some(id(1))
        );
    }

    #[tokio::test]
    async fn exec_command_captures_output() {
        let mgr = UnifiedExecProcessManager::default();
        let pid = mgr.allocate_process_id().await;
        let call_id = "test-call";
        let response = mgr
            .exec_command(
                super::super::ExecCommandRequest {
                    command: vec!["/bin/sh".into(), "-c".into(), "echo hello_pty".into()],
                    process_id: pid,
                    yield_time_ms: 5_000,
                    max_output_tokens: None,
                    workdir: Some(std::env::temp_dir()),
                    tty: false,
                    justification: None,
                    prefix_rule: None,
                },
                call_id,
            )
            .await
            .expect("exec_command should succeed");

        assert!(
            response.output.contains("hello_pty"),
            "output should contain 'hello_pty', got: {}",
            response.output
        );
        // Short command should have exited
        assert!(response.exit_code.is_some());
        assert_eq!(response.exit_code.unwrap(), 0);
        // Exited process should not report a process_id
        assert!(response.process_id.is_none());
    }

    #[tokio::test]
    async fn exec_command_nonzero_exit() {
        let mgr = UnifiedExecProcessManager::default();
        let pid = mgr.allocate_process_id().await;
        let response = mgr
            .exec_command(
                super::super::ExecCommandRequest {
                    command: vec!["/bin/sh".into(), "-c".into(), "exit 42".into()],
                    process_id: pid,
                    yield_time_ms: 5_000,
                    max_output_tokens: None,
                    workdir: Some(std::env::temp_dir()),
                    tty: false,
                    justification: None,
                    prefix_rule: None,
                },
                "test",
            )
            .await
            .expect("exec_command should succeed");

        assert_eq!(response.exit_code, Some(42));
    }

    #[tokio::test]
    async fn write_stdin_to_interactive_shell() {
        let mgr = UnifiedExecProcessManager::default();
        let pid = mgr.allocate_process_id().await;

        // Start an interactive shell
        let response = mgr
            .exec_command(
                super::super::ExecCommandRequest {
                    command: vec!["/bin/sh".into(), "-i".into()],
                    process_id: pid.clone(),
                    yield_time_ms: 2_000,
                    max_output_tokens: None,
                    workdir: Some(std::env::temp_dir()),
                    tty: true,
                    justification: None,
                    prefix_rule: None,
                },
                "test",
            )
            .await
            .expect("exec_command should succeed");

        // Should still be running
        let process_id = response
            .process_id
            .expect("interactive shell should report process_id");

        // Write to stdin
        let write_response = mgr
            .write_stdin(super::super::WriteStdinRequest {
                process_id: &process_id,
                input: "echo mosaic_test_var\n",
                yield_time_ms: 3_000,
                max_output_tokens: None,
            })
            .await
            .expect("write_stdin should succeed");

        assert!(
            write_response.output.contains("mosaic_test_var"),
            "write_stdin output should contain 'mosaic_test_var', got: {}",
            write_response.output
        );

        // Clean up: exit the shell
        let _ = mgr
            .write_stdin(super::super::WriteStdinRequest {
                process_id: &process_id,
                input: "exit\n",
                yield_time_ms: 2_000,
                max_output_tokens: None,
            })
            .await;
    }

    #[tokio::test]
    async fn write_stdin_unknown_process() {
        let mgr = UnifiedExecProcessManager::default();
        let result = mgr
            .write_stdin(super::super::WriteStdinRequest {
                process_id: "nonexistent",
                input: "hello",
                yield_time_ms: 1_000,
                max_output_tokens: None,
            })
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn terminate_all_cleans_up() {
        let mgr = UnifiedExecProcessManager::default();
        let pid = mgr.allocate_process_id().await;

        // Start a long-running process
        let response = mgr
            .exec_command(
                super::super::ExecCommandRequest {
                    command: vec!["/bin/sh".into(), "-c".into(), "sleep 60".into()],
                    process_id: pid,
                    yield_time_ms: 500,
                    max_output_tokens: None,
                    workdir: Some(std::env::temp_dir()),
                    tty: true,
                    justification: None,
                    prefix_rule: None,
                },
                "test",
            )
            .await
            .expect("exec_command should succeed");

        assert!(response.process_id.is_some());

        // Terminate all
        mgr.terminate_all_processes().await;

        let store = mgr.process_store.lock().await;
        assert!(store.processes.is_empty());
        assert!(store.reserved_process_ids.is_empty());
    }

    #[test]
    fn pruning_protects_recent_even_if_exited() {
        let now = Instant::now();
        let id = |n: i32| n.to_string();
        let meta = vec![
            (id(1), now - Duration::from_secs(40), false),
            (id(2), now - Duration::from_secs(30), false),
            (id(3), now - Duration::from_secs(20), true),
            (id(4), now - Duration::from_secs(19), false),
            (id(5), now - Duration::from_secs(18), false),
            (id(6), now - Duration::from_secs(17), false),
            (id(7), now - Duration::from_secs(16), false),
            (id(8), now - Duration::from_secs(15), false),
            (id(9), now - Duration::from_secs(14), false),
            (id(10), now - Duration::from_secs(13), true),
        ];
        // (10) is exited but protected (top 8 by recency); prune LRU outside protected
        assert_eq!(
            UnifiedExecProcessManager::process_id_to_prune(&meta),
            Some(id(1))
        );
    }
}
