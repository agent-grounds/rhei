//! The one way a test opens a native pseudo-terminal: both ends close-on-exec from
//! the call that creates them, so no process keeps it past its own `exec`.
//! §REQ-cross-platform.7.2
//!
//! Tests in this binary spawn on other threads at any moment, and a spawn
//! inherits every descriptor not flagged at that instant, so a flag set after the
//! creation leaves a gap however short. A process that inherits the master through
//! that gap keeps the terminal alive past the test's own close, nothing hangs up,
//! and a run that §FS-rhei-run.3.2 says ends on its lost terminal is never told.

use std::os::fd::{AsRawFd as _, FromRawFd as _, IntoRawFd as _, OwnedFd};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::fcntl::{FcntlArg, FdFlag, OFlag};
use nix::pty::{OpenptyResult, Winsize};
use nix::sys::stat::Mode;

/// `ptsname` answers from a static buffer inside libc, and the form that takes a
/// buffer of its own exists only on Linux. Nothing else in this binary calls it,
/// so holding this is all the exclusion its safety contract asks for.
static PTSNAME: Mutex<()> = Mutex::new(());

/// A pseudo-terminal of `size`, whose two ends are each close-on-exec from the
/// call that created it.
///
/// It is not the caller's controlling terminal, so a hang-up delivers no `SIGHUP`
/// here. A child gets the terminal only by being handed a clone as its stdio. A
/// clone is close-on-exec too, and `Command` installs it over the stdio descriptor
/// in the child alone.
pub fn open_native_terminal(size: &Winsize) -> OpenptyResult {
    let flags = OFlag::O_RDWR | OFlag::O_NOCTTY | OFlag::O_CLOEXEC;
    let master = nix::pty::posix_openpt(flags).expect("open the pty master close-on-exec");
    nix::pty::grantpt(&master).expect("grant the pty slave");
    nix::pty::unlockpt(&master).expect("unlock the pty slave");
    let slave_name = {
        let _sole_caller = PTSNAME.lock().unwrap_or_else(PoisonError::into_inner);
        // SAFETY: `PTSNAME` is held, and the name is copied out before it is released.
        unsafe { nix::pty::ptsname(&master) }.expect("name the pty slave")
    };
    let slave = nix::fcntl::open(slave_name.as_str(), flags, Mode::empty())
        .expect("open the pty slave close-on-exec");
    // SAFETY: `open` just returned this descriptor and nothing else owns it.
    let slave = unsafe { OwnedFd::from_raw_fd(slave) };
    // SAFETY: `into_raw_fd` just gave this descriptor up and nothing else owns it.
    let master = unsafe { OwnedFd::from_raw_fd(master.into_raw_fd()) };

    // SAFETY: `TIOCSWINSZ` reads one `winsize`, and `size` outlives the call.
    let sized = unsafe {
        nix::libc::ioctl(slave.as_raw_fd(), nix::libc::TIOCSWINSZ, std::ptr::from_ref(size))
    };
    assert_eq!(sized, 0, "size the pty: {}", std::io::Error::last_os_error());

    // A platform that dropped the flag would reopen the window without a word, and
    // the failure would show up as a rare hang in whichever test last-closes it.
    // This catches a lost flag and not a late one, which reads the same here; the
    // guard in `suite_isolation_tests.rs` refuses the easy way of writing that.
    for (end, descriptor) in [("master", &master), ("slave", &slave)] {
        let bits = nix::fcntl::fcntl(descriptor.as_raw_fd(), FcntlArg::F_GETFD)
            .expect("read the pty's descriptor flags");
        assert!(
            FdFlag::from_bits_truncate(bits).contains(FdFlag::FD_CLOEXEC),
            "the pty {end} must be close-on-exec (§REQ-cross-platform.7.2)"
        );
    }
    OpenptyResult { master, slave }
}

/// The binary's margin as a bound a test waits on itself, so its size stays decided
/// in one place. §REQ-cross-platform.8
fn margin() -> Duration {
    let seconds = super::FIXTURE_MARGIN.strip_suffix('s').and_then(|whole| whole.parse().ok());
    Duration::from_secs(seconds.expect("FIXTURE_MARGIN is a whole number of seconds"))
}

/// Writes to `slave` until the terminal answers that it has hung up, within `bound`.
///
/// A process already spawned holds the master until its `exec`, flagged or not, so
/// the hang-up can come a moment after the last close this test can see
/// (§REQ-cross-platform.7.2). `slave` must be non-blocking: a blocking write to a
/// terminal that never hung up fills its buffer and stays there, past any bound.
fn hang_up_within(slave: &OwnedFd, bound: Duration) -> Result<(), String> {
    let deadline = Instant::now() + bound;
    loop {
        match nix::unistd::write(slave, b"x") {
            Err(Errno::EIO) => return Ok(()),
            Ok(_) | Err(Errno::EAGAIN) if Instant::now() >= deadline => {
                return Err(format!(
                    "the terminal did not hang up within {bound:?}, so something still holds \
                     its master"
                ));
            }
            Ok(_) | Err(Errno::EAGAIN) => std::thread::sleep(Duration::from_millis(1)),
            Err(other) => {
                return Err(format!("a write to a hung-up pty should fail with EIO: {other}"));
            }
        }
    }
}

/// The first part of the measure of §REQ-cross-platform.7.2: a process spawned after
/// the terminal was opened, and still alive when its master closes, does not stop
/// the hang-up.
///
/// The spawn lands after the open every time and every stdio of the process is
/// `/dev/null`, so only what it inherited can hold the master. Flagged, that copy
/// lasts until its `exec`, which is why the verdict waits for the hang-up rather
/// than judging the statement after the close. Unflagged, it lasts the process's
/// 300s, past the margin, so a helper that lost the flag fails this every time
/// rather than rarely.
// §REQ-cross-platform.7.2 §FS-rhei-run.3.2
#[test]
fn a_process_spawned_after_the_terminal_opened_does_not_keep_it_from_hanging_up() {
    use std::process::{Command, Stdio};

    let size = Winsize { ws_row: 24, ws_col: 80, ws_xpixel: 0, ws_ypixel: 0 };
    let pty = open_native_terminal(&size);
    // On the open file description, so clones would share it: this slave goes to no
    // child, which is why it is set here and not in the helper.
    let bits = nix::fcntl::fcntl(pty.slave.as_raw_fd(), FcntlArg::F_GETFL)
        .expect("read the slave's status flags");
    let nonblocking = OFlag::from_bits_truncate(bits) | OFlag::O_NONBLOCK;
    nix::fcntl::fcntl(pty.slave.as_raw_fd(), FcntlArg::F_SETFL(nonblocking))
        .expect("make the slave non-blocking");
    let bound = margin();

    let mut witness = Command::new("sleep")
        .arg("300")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("sleep should start");

    drop(pty.master);
    let hung_up = hang_up_within(&pty.slave, bound);

    // Everything the verdict needs is read before the kill, and nothing before the
    // reap can panic, so the process is gone whichever way the verdict goes.
    let outlived_the_close = matches!(witness.try_wait(), Ok(None));
    let _ = witness.kill();
    let _ = witness.wait();

    assert!(outlived_the_close, "the witness must outlive the master's close");
    if let Err(failure) = hung_up {
        panic!("{failure}");
    }
}
