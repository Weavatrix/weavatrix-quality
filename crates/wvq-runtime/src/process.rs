//! Bounded process spawn: deadline, output cap, cancel. No shell.
//!
//! Timeout and cancel kill the process *tree*: Unix process group, Windows job
//! object. `Child::kill` on the parent is not enough for Vitest or Chromium.

use std::io::{self, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use process_wrap::std::{ChildWrapper, CommandWrap};
#[cfg(windows)]
use process_wrap::std::JobObject;
#[cfg(unix)]
use process_wrap::std::ProcessGroup;

use crate::normalize::RuntimeError;

/// Spawned command whose kill covers descendants.
pub(crate) type TreeChild = Box<dyn ChildWrapper>;

/// Spawn `command` in a Unix process group or a Windows job object.
pub(crate) fn spawn_tree(command: Command) -> io::Result<TreeChild> {
    let mut wrap = CommandWrap::from(command);
    #[cfg(unix)]
    wrap.wrap(ProcessGroup::leader());
    #[cfg(windows)]
    wrap.wrap(JobObject);
    wrap.spawn()
}

/// Caps applied to every registered spawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessLimits {
    /// Wall-clock timeout.
    pub deadline: Duration,
    /// Combined stdout+stderr cap.
    pub max_output_bytes: usize,
}

/// Captured process output (already capped).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawExecution {
    /// Exit code if the process exited on its own.
    pub status_code: Option<i32>,
    /// Stdout bytes.
    pub stdout: Vec<u8>,
    /// Stderr bytes.
    pub stderr: Vec<u8>,
}

/// Spawn `program` + `args` with limits. `program` comes from the registry.
///
/// # Errors
///
/// [`RuntimeError::Spawn`], [`RuntimeError::DeadlineExceeded`],
/// [`RuntimeError::OutputLimit`], or [`RuntimeError::Cancelled`].
pub fn run_bounded(
    program: &str,
    args: &[String],
    cwd: &Path,
    limits: &ProcessLimits,
    cancel: &AtomicBool,
) -> Result<RawExecution, RuntimeError> {
    if limits.max_output_bytes == 0 || limits.deadline.is_zero() {
        return Err(RuntimeError::InvalidArg(
            "deadline and max_output_bytes must be positive".into(),
        ));
    }
    if cancel.load(Ordering::SeqCst) {
        return Err(RuntimeError::Cancelled);
    }
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = spawn_tree(command).map_err(|err| RuntimeError::Spawn(err.to_string()))?;
    let stdout = child.stdout().take();
    let stderr = child.stderr().take();
    let (tx, rx) = mpsc::channel::<StreamChunk>();
    let mut readers = 0_u8;
    if stdout.is_some() {
        readers = readers.saturating_add(1);
        spawn_reader(stdout, true, tx.clone());
    }
    if stderr.is_some() {
        readers = readers.saturating_add(1);
        spawn_reader(stderr, false, tx);
    } else {
        drop(tx);
    }
    collect_child(child.as_mut(), &rx, limits, cancel, readers)
}

enum StreamChunk {
    Out(Vec<u8>),
    Err(Vec<u8>),
    Done,
}

fn spawn_reader<R>(stream: Option<R>, is_out: bool, tx: mpsc::Sender<StreamChunk>)
where
    R: Read + Send + 'static,
{
    let Some(mut stream) = stream else {
        return;
    };
    thread::spawn(move || {
        let mut buf = [0_u8; 4096];
        loop {
            match stream.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let chunk = buf[..n].to_vec();
                    let msg = if is_out {
                        StreamChunk::Out(chunk)
                    } else {
                        StreamChunk::Err(chunk)
                    };
                    if tx.send(msg).is_err() {
                        break;
                    }
                }
            }
        }
        let _ = tx.send(StreamChunk::Done);
    });
}

fn collect_child(
    child: &mut dyn ChildWrapper,
    rx: &mpsc::Receiver<StreamChunk>,
    limits: &ProcessLimits,
    cancel: &AtomicBool,
    expected_readers: u8,
) -> Result<RawExecution, RuntimeError> {
    let started = Instant::now();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut done_readers = 0_u8;
    loop {
        if cancel.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(RuntimeError::Cancelled);
        }
        if started.elapsed() >= limits.deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(RuntimeError::DeadlineExceeded);
        }
        let remaining = limits.deadline.saturating_sub(started.elapsed());
        let wait = remaining.min(Duration::from_millis(20));
        match rx.recv_timeout(wait) {
            Ok(StreamChunk::Out(chunk)) => {
                append_capped(&mut stdout, &stderr, &chunk, limits, child)?;
            }
            Ok(StreamChunk::Err(chunk)) => {
                append_capped(&mut stderr, &stdout, &chunk, limits, child)?;
            }
            Ok(StreamChunk::Done) => {
                done_readers = done_readers.saturating_add(1);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                done_readers = 2;
            }
        }
        if let Some(status) = child.try_wait().map_err(|err| RuntimeError::Spawn(err.to_string()))?
            && done_readers >= expected_readers
        {
            return Ok(RawExecution {
                status_code: status.code(),
                stdout,
                stderr,
            });
        }
    }
}

fn append_capped(
    dest: &mut Vec<u8>,
    other: &[u8],
    chunk: &[u8],
    limits: &ProcessLimits,
    child: &mut dyn ChildWrapper,
) -> Result<(), RuntimeError> {
    let used = dest.len().saturating_add(other.len()).saturating_add(chunk.len());
    if used > limits.max_output_bytes {
        let _ = child.kill();
        let _ = child.wait();
        return Err(RuntimeError::OutputLimit {
            max: limits.max_output_bytes,
        });
    }
    dest.extend_from_slice(chunk);
    Ok(())
}
