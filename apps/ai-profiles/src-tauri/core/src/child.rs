//! Running a program for its output, and giving up on it after a while: a
//! `claude` that hangs mustn't hold a request, or a lock, forever.

use std::io::{self, Read, Write};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// What a program that finished in time wrote, and how it ended.
pub struct Finished {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Run `command` with `input` on its stdin, and its output collected.
/// `Ok(None)` when it took longer than `timeout`: it is killed, and waited
/// for, so nothing is left running.
pub fn run_within(
    command: &mut Command,
    input: Vec<u8>,
    timeout: Duration,
) -> io::Result<Option<Finished>> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let collect = |pipe: Option<Box<dyn Read + Send>>| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            bytes
        })
    };
    let stdout = collect(child.stdout.take().map(|pipe| Box::new(pipe) as _));
    let stderr = collect(child.stderr.take().map(|pipe| Box::new(pipe) as _));
    // From a thread of its own, so a program that doesn't read it can't hold
    // this up past the deadline. One that exits without reading all of it
    // closes the pipe: what it wrote says why.
    if let Some(mut stdin) = child.stdin.take() {
        thread::spawn(move || {
            let _ = stdin.write_all(&input);
        });
    }
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        thread::sleep(Duration::from_millis(20));
    };
    // With the program gone its pipes close, so these finish.
    let stdout = stdout.join().unwrap_or_default();
    let stderr = stderr.join().unwrap_or_default();
    Ok(status.map(|status| Finished {
        status,
        stdout,
        stderr,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_what_a_program_writes() {
        let finished = run_within(
            Command::new("/bin/sh").args(["-c", "cat; echo oops >&2"]),
            b"hello".to_vec(),
            Duration::from_secs(10),
        )
        .unwrap()
        .unwrap();
        assert!(finished.status.success());
        assert_eq!(finished.stdout, b"hello");
        assert_eq!(finished.stderr, b"oops\n");
    }

    #[test]
    fn gives_up_on_a_program_that_takes_too_long() {
        let started = Instant::now();
        let finished = run_within(
            Command::new("/bin/sh").args(["-c", "exec sleep 30"]),
            Vec::new(),
            Duration::from_millis(200),
        )
        .unwrap();
        assert!(finished.is_none());
        assert!(started.elapsed() < Duration::from_secs(10));
    }
}
