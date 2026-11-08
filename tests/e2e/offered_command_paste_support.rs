//! Reading a command a diagnostic offers off the physical lines it was printed
//! on, and pasting that one line into a shell the way a reader would.
//!
//! The rule under test is about physical lines, so nothing here rejoins them:
//! what the binary *meant* is read from the soft-wrap-undone text, what a
//! reader *gets* is read from the one raw line that holds the lead-in, and the
//! two must be the same command. §FS-rhei-errors.2.1

#[cfg(unix)]
use std::path::Path;
#[cfg(unix)]
use std::process::Output;

use super::rendered_stderr::undo_soft_wrap;

/// What follows `lead_in` on the first line of `text` that holds it, cut at
/// `until` when that follows on the same line, and trimmed: the command as the
/// binary meant it, read from the soft-wrap-undone text.
fn following(text: &str, lead_in: &str, until: Option<&str>) -> Option<String> {
    text.lines().find_map(|line| {
        let (_, rest) = line.split_once(lead_in.trim_end())?;
        Some(cut(rest, until))
    })
}

fn cut(rest: &str, until: Option<&str>) -> String {
    let rest = until.and_then(|until| rest.split_once(until)).map_or(rest, |(cut, _)| cut);
    rest.trim().to_owned()
}

/// The physical line a reader pastes to run `meant`: the one its first word
/// starts on, after the lead-in's last word, or the line after that word where
/// the render broke right behind it. Only the lead-in's last word is matched,
/// because the rule is about the command's line, not the sentence's: a lead-in
/// broken in its middle is prose wrapping, which §FS-rhei-errors.2 allows.
fn pasted_line(raw: &str, lead_in: &str, meant: &str, until: Option<&str>) -> String {
    let tail = lead_in.split_whitespace().last().unwrap_or(lead_in);
    let first_word = meant.split_whitespace().next().unwrap_or(meant);
    let lines: Vec<&str> = raw.lines().collect();
    for (at, line) in lines.iter().enumerate() {
        let Some((_, rest)) = line.split_once(tail) else { continue };
        if rest.trim().is_empty() {
            let next = lines.get(at + 1).copied().unwrap_or_default();
            return cut(next.trim_start_matches([' ', '\u{2502}']), until);
        }
        if rest.trim_start().starts_with(first_word) {
            return cut(rest, until);
        }
    }
    String::new()
}

/// The width of the soft-wrap-undone line that holds `lead_in`: how wide the
/// line is when nothing breaks it.
fn whole_width(text: &str, lead_in: &str) -> Option<usize> {
    let lead_in = lead_in.trim_end();
    text.lines().find(|line| line.contains(lead_in)).map(|line| line.chars().count())
}

/// The command offered after `lead_in` in a rendered diagnostic, required to
/// sit whole on the physical line that holds the lead-in, so that pasting that
/// line alone runs it. §FS-rhei-errors.2.1
///
/// `wider_than` is the fixture's own check that it pins something: the line, if
/// nothing broke it, must be wider than that many columns, so a renderer that
/// wraps it at that width splits the command. Pass `0` for a control.
pub(super) fn offered_whole(
    rendered: &str,
    lead_in: &str,
    until: Option<&str>,
    wider_than: usize,
) -> String {
    let raw = rendered.replace('\r', "");
    let unwrapped = undo_soft_wrap(&raw);
    let meant = following(&unwrapped, lead_in, until).unwrap_or_else(|| {
        panic!("the diagnostic offers no command after {lead_in:?}, so this proves nothing:\n{raw}")
    });
    assert!(!meant.is_empty(), "the offer after {lead_in:?} names no command:\n{raw}");
    let width = whole_width(&unwrapped, lead_in).unwrap_or_default();
    assert!(
        width > wider_than,
        "fixture: the line offering `{meant}` is {width} columns whole, which a \
         {wider_than}-column render never has to break, so this pins nothing:\n{raw}"
    );
    let pasted = pasted_line(&raw, lead_in, &meant, until);
    assert_eq!(
        pasted, meant,
        "§FS-rhei-errors.2.1: the physical line that `{meant}` starts on must hold all of \
         it, so pasting that line runs it; the render split it:\n{raw}"
    );
    pasted
}

/// Paste `line` into a fresh POSIX shell from `cwd`, as a reader who copied
/// the one line would. A shell, not `rhei_command`, so the run-context removal
/// is applied here. §REQ-test-isolation.7
///
/// Unix only: the commands under test are the POSIX spellings the binary
/// prints there, and `sh` is the shell a pasted line lands in. On Windows the
/// same-line check runs and the paste does not. §REQ-cross-platform.2
#[cfg(unix)]
pub(super) fn paste(cwd: &Path, line: &str) -> Output {
    let mut shell = std::process::Command::new("sh");
    shell.arg("-c").arg(line).current_dir(cwd).stdin(std::process::Stdio::null());
    super::git_env::scrub_rhei_env(&mut shell);
    shell.output().expect("sh should run the pasted line")
}

/// [`paste`], required to succeed, returning what it printed.
#[cfg(unix)]
pub(super) fn paste_ok(cwd: &Path, line: &str) -> String {
    let out = paste(cwd, line);
    // The shell's output, not rhei's: nothing here is a rendered diagnostic.
    let mut printed = out.stdout.clone();
    printed.extend_from_slice(&out.stderr);
    let said = String::from_utf8_lossy(&printed).into_owned();
    assert!(out.status.success(), "the pasted line `{line}` failed in {}:\n{said}", cwd.display());
    said
}

/// `rhei <args>` from `cwd` with stdout and stderr on one native terminal
/// `columns` wide, read to its end with the line discipline's carriage returns
/// left in: [`offered_whole`] reads physical lines and strips them itself.
#[cfg(unix)]
pub(super) fn rendered_on_terminal(
    home: &Path,
    cwd: &Path,
    columns: u16,
    args: &[&str],
) -> (std::process::ExitStatus, String) {
    use std::io::Read as _;
    use std::os::fd::OwnedFd;

    let size = nix::pty::Winsize { ws_row: 50, ws_col: columns, ws_xpixel: 0, ws_ypixel: 0 };
    // Read to end of file, so a leaked slave would hold it off. §REQ-cross-platform.7.2
    let pty = super::native_pty_support::open_native_terminal(&size);
    let mut command = super::rhei_command(home);
    let stdout: OwnedFd = pty.slave.try_clone().expect("clone pty slave");
    let stderr: OwnedFd = pty.slave.try_clone().expect("clone pty slave");
    command
        .current_dir(cwd)
        .env("NO_COLOR", "1")
        .env("TERM", "xterm")
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::from(stdout))
        .stderr(std::process::Stdio::from(stderr));
    let mut child = command.spawn().expect("rhei should start on the terminal");
    drop(command);
    drop(pty.slave);
    let mut bytes = Vec::new();
    let mut master = std::fs::File::from(pty.master);
    let mut chunk = [0_u8; 8_192];
    while let Ok(count) = master.read(&mut chunk) {
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    let status = child.wait().expect("wait for rhei");
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

/// Make `path` unreadable, returning `false` where the current user reads it
/// anyway (root, or a filesystem that ignores modes), so the caller skips the
/// case rather than pass it without reaching the refusal.
#[cfg(unix)]
pub(super) fn lock(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o000)).expect("chmod 000");
    if std::fs::read(path).is_ok() {
        unlock(path);
        eprintln!("skipped: {} is still readable at mode 000 (running as root?)", path.display());
        return false;
    }
    true
}

/// Give `path` back its permissions, so the fixture directory can be removed.
#[cfg(unix)]
pub(super) fn unlock(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).expect("chmod 600");
}
