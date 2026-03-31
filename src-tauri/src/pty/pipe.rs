//! Spawn a non-interactive process using regular pipes for stdin/stdout/stderr.

use std::collections::HashMap;
use std::io;
use std::io::ErrorKind;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::Result;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::pty::process::{ChildTerminator, ProcessHandle, SpawnedProcess};

struct PipeChildTerminator {
    #[cfg(unix)]
    process_group_id: u32,
}

impl ChildTerminator for PipeChildTerminator {
    fn kill(&mut self) -> io::Result<()> {
        #[cfg(unix)]
        {
            crate::pty::process_group::kill_process_group(self.process_group_id)
        }
        #[cfg(not(unix))]
        {
            Ok(())
        }
    }
}

async fn read_output_stream<R: AsyncRead + Unpin>(
    mut reader: R,
    output_tx: broadcast::Sender<Vec<u8>>,
) {
    let mut buf = vec![0u8; 8_192];
    loop {
        match reader.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                let _ = output_tx.send(buf[..n].to_vec());
            }
            Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
}

/// Spawn a process using regular pipes.
pub async fn spawn_process(
    program: &str,
    args: &[String],
    cwd: &Path,
    env: &HashMap<String, String>,
    _arg0: &Option<String>,
) -> Result<SpawnedProcess> {
    if program.is_empty() {
        anyhow::bail!("missing program for pipe spawn");
    }

    let mut command = Command::new(program);
    #[cfg(target_os = "linux")]
    let parent_pid = unsafe { libc::getpid() };
    #[cfg(unix)]
    unsafe {
        command.pre_exec(move || {
            crate::pty::process_group::detach_from_tty()?;
            #[cfg(target_os = "linux")]
            crate::pty::process_group::set_parent_death_signal(parent_pid)?;
            Ok(())
        });
    }
    command.current_dir(cwd);
    command.env_clear();
    for (key, value) in env {
        command.env(key, value);
    }
    for arg in args {
        command.arg(arg);
    }
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let mut child = command.spawn()?;
    let pid = child
        .id()
        .ok_or_else(|| io::Error::other("missing child pid"))?;
    #[cfg(unix)]
    let process_group_id = pid;

    let stdin = child.stdin.take();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let (writer_tx, mut writer_rx) = mpsc::channel::<Vec<u8>>(128);
    let (output_tx, _) = broadcast::channel::<Vec<u8>>(256);
    let initial_output_rx = output_tx.subscribe();

    let writer_handle = if let Some(stdin) = stdin {
        let writer = Arc::new(tokio::sync::Mutex::new(stdin));
        tokio::spawn(async move {
            while let Some(bytes) = writer_rx.recv().await {
                let mut guard = writer.lock().await;
                let _ = guard.write_all(&bytes).await;
                let _ = guard.flush().await;
            }
        })
    } else {
        drop(writer_rx);
        tokio::spawn(async {})
    };

    let stdout_handle = stdout.map(|s| {
        let tx = output_tx.clone();
        tokio::spawn(async move {
            read_output_stream(BufReader::new(s), tx).await;
        })
    });
    let stderr_handle = stderr.map(|s| {
        let tx = output_tx.clone();
        tokio::spawn(async move {
            read_output_stream(BufReader::new(s), tx).await;
        })
    });
    let mut reader_abort_handles = Vec::new();
    if let Some(h) = stdout_handle.as_ref() {
        reader_abort_handles.push(h.abort_handle());
    }
    if let Some(h) = stderr_handle.as_ref() {
        reader_abort_handles.push(h.abort_handle());
    }
    let reader_handle = tokio::spawn(async move {
        if let Some(h) = stdout_handle {
            let _ = h.await;
        }
        if let Some(h) = stderr_handle {
            let _ = h.await;
        }
    });

    let (exit_tx, exit_rx) = oneshot::channel::<i32>();
    let exit_status = Arc::new(AtomicBool::new(false));
    let wait_exit_status = Arc::clone(&exit_status);
    let exit_code = Arc::new(StdMutex::new(None));
    let wait_exit_code = Arc::clone(&exit_code);
    let wait_handle: JoinHandle<()> = tokio::spawn(async move {
        let code = match child.wait().await {
            Ok(status) => status.code().unwrap_or(-1),
            Err(_) => -1,
        };
        wait_exit_status.store(true, std::sync::atomic::Ordering::SeqCst);
        if let Ok(mut guard) = wait_exit_code.lock() {
            *guard = Some(code);
        }
        let _ = exit_tx.send(code);
    });

    let (handle, output_rx) = ProcessHandle::new(
        writer_tx,
        output_tx,
        initial_output_rx,
        Box::new(PipeChildTerminator {
            #[cfg(unix)]
            process_group_id,
        }),
        reader_handle,
        reader_abort_handles,
        writer_handle,
        wait_handle,
        exit_status,
        exit_code,
        None,
    );

    Ok(SpawnedProcess {
        session: handle,
        output_rx,
        exit_rx,
    })
}

/// Spawn a process using regular pipes, but close stdin immediately.
pub async fn spawn_process_no_stdin(
    program: &str,
    args: &[String],
    cwd: &Path,
    env: &HashMap<String, String>,
    arg0: &Option<String>,
) -> Result<SpawnedProcess> {
    // Reuse spawn_process; the stdin channel simply won't be written to.
    spawn_process(program, args, cwd, env, arg0).await
}
