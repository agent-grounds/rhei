//! Fixtures and process helpers for attended operator recovery scenarios.
//! The scenarios themselves are black-box proofs of §FS-rhei-transition-cmd.6
//! and §FS-rhei-recover.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
#[cfg(unix)]
use std::process::{Command, Stdio};

use super::*;

pub const FORCE_MACHINE: &str = r#"name: operator-recovery
version: 1
states:
  human-gate:
    initial: true
    gating: true
    description: Human decision
  implement:
    description: Work
  completed:
    final: true
    description: Done
  cancelled:
    final: true
    description: Abandoned
transitions:
  - from: human-gate
    to: completed
  - from: implement
    to: cancelled
"#;

pub const GATE_PLAN: &str = r#"# Rhei: Forced Recovery

## Tasks

### Task 1: Repair me
**State:** human-gate
"#;

pub struct ForceFixture {
    pub dir: TestDir,
    pub plan: PathBuf,
    pub machine: PathBuf,
}

pub fn force_fixture(prefix: &str, plan: &str, machine: &str) -> ForceFixture {
    let dir = unique_temp_dir(prefix);
    let plan = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine = write_fixture_file(&dir, "states.yaml", machine);
    let validation = run_cli("validate", &plan, &machine, &[]);
    assert_success(&validation);
    ForceFixture { dir, plan, machine }
}

pub fn force_args<'a>(task: &'a str, from: &'a str, to: &'a str, reason: &'a str) -> Vec<&'a str> {
    vec!["--task", task, "--from", from, "--to", to, "--force", "--reason", reason]
}

pub fn run_force_noninteractive(
    fixture: &ForceFixture,
    task: &str,
    from: &str,
    to: &str,
    reason: &str,
    extra: &[&str],
) -> CliRun {
    let mut args = force_args(task, from, to, reason);
    args.extend_from_slice(extra);
    run_cli("transition", &fixture.plan, &fixture.machine, &args)
}

#[derive(Debug)]
pub struct TerminalRun {
    pub status: ExitStatus,
    pub transcript: String,
}

#[cfg(unix)]
pub fn run_force_in_terminal(
    fixture: &ForceFixture,
    task: &str,
    from: &str,
    to: &str,
    reason: &str,
    result: Option<&str>,
) -> TerminalRun {
    let mut command = rhei_command(fixture.dir.join("home"));
    command.arg("--state-machine").arg(&fixture.machine).arg("transition").arg(&fixture.plan);
    for arg in force_args(task, from, to, reason) {
        command.arg(arg);
    }
    if let Some(result) = result {
        command.arg("--result").arg(result);
    }
    run_in_terminal(command, &format!("force plan.{task} {from} -> {to}\n"))
}

#[cfg(unix)]
pub fn run_recover_in_terminal(root: &Path, confirmation: &str) -> TerminalRun {
    let mut command = rhei_command(root.join("home"));
    command.arg("recover").arg(root);
    run_in_terminal(command, &format!("{confirmation}\n"))
}

#[cfg(unix)]
fn run_in_terminal(mut command: Command, response: &str) -> TerminalRun {
    use std::io::{Read, Write};
    use std::os::fd::AsRawFd;
    use std::time::{Duration, Instant};

    // Keep exact diagnostics independent of terminal wrapping. §FS-rhei-transition-cmd.6
    let size = nix::pty::Winsize { ws_row: 40, ws_col: 240, ws_xpixel: 0, ws_ypixel: 0 };
    let pty = nix::pty::openpty(Some(&size), None).expect("open pseudo-terminal");
    for fd in [pty.master.as_raw_fd(), pty.slave.as_raw_fd()] {
        nix::fcntl::fcntl(fd, nix::fcntl::FcntlArg::F_SETFD(nix::fcntl::FdFlag::FD_CLOEXEC))
            .expect("keep pseudo-terminal handles out of the child");
    }
    let mut master = fs::File::from(pty.master);
    let slave = fs::File::from(pty.slave);
    command
        .stdin(Stdio::from(slave.try_clone().expect("clone pty slave for stdin")))
        .stdout(Stdio::from(slave.try_clone().expect("clone pty slave for stdout")))
        .stderr(Stdio::from(slave.try_clone().expect("clone pty slave for stderr")));
    let mut child = command.spawn().expect("spawn rhei in pseudo-terminal");
    drop(slave);
    master.write_all(response.as_bytes()).expect("write typed confirmation");
    master.flush().expect("flush typed confirmation");

    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll rhei") {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().expect("kill hung rhei test process");
            let _ = child.wait();
            panic!("rhei did not finish after typed confirmation");
        }
        std::thread::sleep(Duration::from_millis(10));
    };

    let bits = nix::fcntl::fcntl(master.as_raw_fd(), nix::fcntl::FcntlArg::F_GETFL)
        .expect("read pseudo-terminal flags");
    let flags = nix::fcntl::OFlag::from_bits_truncate(bits) | nix::fcntl::OFlag::O_NONBLOCK;
    nix::fcntl::fcntl(master.as_raw_fd(), nix::fcntl::FcntlArg::F_SETFL(flags))
        .expect("make pseudo-terminal transcript non-blocking");

    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        match master.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => bytes.extend_from_slice(&chunk[..read]),
            Err(err) if err.raw_os_error() == Some(nix::errno::Errno::EIO as i32) => break,
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(err) => panic!("read pseudo-terminal transcript: {err}"),
        }
    }
    TerminalRun { status, transcript: String::from_utf8_lossy(&bytes).into_owned() }
}

pub fn artifact_snapshot(root: &Path, plan: &Path) -> Vec<(PathBuf, Option<Vec<u8>>)> {
    [
        plan.to_path_buf(),
        root.join("runtime/results/plan.1.md"),
        root.join("runtime/state-transitions.log"),
        root.join("runtime/transitions.log"),
        root.join(".rhei/forced-recovery.json"),
    ]
    .into_iter()
    .map(|path| {
        let bytes = match fs::read(&path) {
            Ok(bytes) => Some(bytes),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
            Err(err) => panic!("read {}: {err}", path.display()),
        };
        (path, bytes)
    })
    .collect()
}

pub fn assert_artifacts_unchanged(before: &[(PathBuf, Option<Vec<u8>>)]) {
    for (path, expected) in before {
        let actual = match fs::read(path) {
            Ok(bytes) => Some(bytes),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
            Err(err) => panic!("read {}: {err}", path.display()),
        };
        assert_eq!(&actual, expected, "refusal changed {}", path.display());
    }
}

pub fn decode_base64url(value: &str) -> Vec<u8> {
    fn digit(byte: u8) -> u8 {
        match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => panic!("invalid base64url byte {byte:?}"),
        }
    }

    let mut bits = 0_u32;
    let mut held = 0_u8;
    let mut decoded = Vec::new();
    for byte in value.bytes() {
        bits = (bits << 6) | u32::from(digit(byte));
        held += 6;
        if held >= 8 {
            held -= 8;
            decoded.push((bits >> held) as u8);
            bits &= (1_u32 << held).saturating_sub(1);
        }
    }
    assert!(held < 6 && bits == 0, "non-canonical base64url tail");
    decoded
}

pub fn encode_base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut encoded = String::new();
    for chunk in bytes.chunks(3) {
        let value = (u32::from(chunk[0]) << 16)
            | (chunk.get(1).copied().map(u32::from).unwrap_or(0) << 8)
            | chunk.get(2).copied().map(u32::from).unwrap_or(0);
        encoded.push(ALPHABET[((value >> 18) & 63) as usize] as char);
        encoded.push(ALPHABET[((value >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            encoded.push(ALPHABET[((value >> 6) & 63) as usize] as char);
        }
        if chunk.len() > 2 {
            encoded.push(ALPHABET[(value & 63) as usize] as char);
        }
    }
    encoded
}
