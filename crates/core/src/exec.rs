//! Running external tools (`docker`, `ollama`, `snap`, …) for the probes: with a timeout,
//! never interactive, and with "tool not installed" as a normal outcome rather than an
//! error. Nothing here ever runs a command that changes the system (R1).

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct Output {
    pub stdout: String,
    pub stderr: String,
    pub status: i32,
}

impl Output {
    pub fn ok(&self) -> bool {
        self.status == 0
    }
}

/// Run `program args…` and capture its output. Returns `None` when the program is not
/// installed, could not be started, or exceeded `timeout`.
pub fn run(program: &str, args: &[&str], timeout: Duration) -> Option<Output> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let mut stderr = child.stderr.take()?;
    let reader = std::thread::spawn(move || {
        let mut out = String::new();
        let mut err = String::new();
        let _ = stdout.read_to_string(&mut out);
        let _ = stderr.read_to_string(&mut err);
        (out, err)
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code().unwrap_or(-1),
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(_) => return None,
        }
    };
    let (stdout, stderr) = reader.join().ok()?;
    Some(Output {
        stdout,
        stderr,
        status,
    })
}

/// Whether `program` can be found on `PATH`.
pub fn installed(program: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| {
        let candidate = dir.join(program);
        candidate.is_file()
    })
}

pub const SHORT: Duration = Duration::from_secs(10);
pub const LONG: Duration = Duration::from_secs(60);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_and_times_out() {
        let out = run("sh", &["-c", "echo hi; echo err >&2; exit 3"], SHORT).unwrap();
        assert_eq!(out.stdout.trim(), "hi");
        assert_eq!(out.stderr.trim(), "err");
        assert_eq!(out.status, 3);
        assert!(run("definitely-not-a-program-xyz", &[], SHORT).is_none());
        assert!(run("sleep", &["5"], Duration::from_millis(100)).is_none());
        assert!(installed("sh"));
        assert!(!installed("definitely-not-a-program-xyz"));
    }
}
