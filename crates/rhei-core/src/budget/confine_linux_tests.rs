//! Containment is proved by breaching it: every case runs the same program
//! twice — once confined and once as an ordinary child — and asserts the
//! unconfined control *succeeds* at what the confined launch must not do. A
//! confinement that silently stopped working would make the control and the
//! confined run agree, and the case would fail.
//! §FS-rhei-budgets.6.4 §AR-neural-admission.5

use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

/// The interpreter the confined child runs, resolved to the real executable
/// rather than a launcher on `PATH`, with the directories it needs to start.
struct Interpreter {
    executable: PathBuf,
    read_only: Vec<PathBuf>,
}

fn interpreter() -> Interpreter {
    let probe = Command::new("python3")
        .args(["-c", "import sys;print(sys.executable);print(sys.base_prefix);print(sys.prefix)"])
        .output()
        .expect("python3 is what this repository's fixtures already run under");
    assert!(probe.status.success(), "python3 -c failed: {probe:?}");
    let text = String::from_utf8(probe.stdout).expect("python paths are utf-8");
    let mut lines = text.lines().map(PathBuf::from);
    let executable =
        lines.next().expect("sys.executable").canonicalize().expect("real interpreter path");
    let mut read_only: Vec<PathBuf> = ["/usr", "/lib", "/lib64", "/bin", "/sbin", "/etc"]
        .into_iter()
        .map(PathBuf::from)
        .chain(lines)
        .filter(|path| path.exists())
        .collect();
    // `--ro-bind` refuses a destination that is already covered, so keep the
    // interpreter's own prefixes only when they add something.
    read_only.sort();
    read_only.dedup();
    read_only.retain(|path| !path.starts_with("/usr") || path == Path::new("/usr"));
    Interpreter { executable, read_only }
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let base = std::env::temp_dir().join(format!(
            "rhei-confine-{name}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&base).expect("scratch directory");
        Self(base.canonicalize().expect("canonical scratch"))
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The same launch, confined and unconfined, so one case can state both
/// halves of the containment claim.
fn launch(
    interpreter: &Interpreter,
    work: &Path,
    source: &str,
    broker: Option<PathBuf>,
) -> ConfinedLaunch {
    ConfinedLaunch {
        program: interpreter.executable.clone(),
        argv: vec!["-I".into(), "-c".into(), source.into()],
        read_only: interpreter.read_only.clone(),
        work: work.to_path_buf(),
        broker_socket: broker,
        environment: BTreeMap::new(),
    }
}

fn unconfined(interpreter: &Interpreter, work: &Path, source: &str) -> Command {
    let mut cmd = Command::new(&interpreter.executable);
    cmd.args(["-I", "-c", source]).current_dir(work).stdin(Stdio::null());
    cmd
}

fn confinement() -> LinuxConfinement {
    LinuxConfinement::resolve().expect(
        "these cases prove Linux confinement, so a host without bubblewrap or unprivileged \
         user namespaces cannot run them; install bubblewrap to reproduce",
    )
}

/// `--unshare-net` gives the child a network namespace holding one loopback
/// device of its own, so an address the host is listening on is not routable
/// from inside — egress denial is the kernel's, not a filter's.
/// §FS-rhei-budgets.6.4
#[test]
fn a_confined_launch_cannot_reach_a_listener_the_host_can() {
    let interpreter = interpreter();
    let work = Scratch::new("egress");
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("host listener");
    let port = listener.local_addr().expect("listener address").port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let _ = stream.write_all(b"host\n");
        }
    });
    let source = format!(
        "import socket,sys\n\
         s=socket.create_connection(('127.0.0.1',{port}),3)\n\
         sys.stdout.write(s.recv(16).decode())\n"
    );

    let control = unconfined(&interpreter, work.path(), &source).output().expect("control run");
    assert!(
        control.status.success() && String::from_utf8_lossy(&control.stdout).contains("host"),
        "the control must reach the listener, or this case proves nothing: {control:?}"
    );

    let confined = confinement()
        .command(&launch(&interpreter, work.path(), &source, None))
        .expect("confined command")
        .stdin(Stdio::null())
        .output()
        .expect("confined run");

    assert!(
        !confined.status.success(),
        "a confined launch reached a host listener: {}",
        String::from_utf8_lossy(&confined.stdout)
    );
}

/// A credential directory that is not bound is unreachable rather than merely
/// unset: the child's mount namespace does not contain it at all.
/// §FS-rhei-budgets.6.4
#[test]
fn a_confined_launch_cannot_read_a_credential_it_was_not_given() {
    let interpreter = interpreter();
    let secrets = Scratch::new("secrets");
    let work = Scratch::new("credential-work");
    let credential = secrets.path().join("provider-key");
    std::fs::write(&credential, "sk-not-for-the-child\n").expect("write credential");
    let source = format!(
        "import sys\nsys.stdout.write(open({:?}).read())\n",
        credential.display().to_string()
    );

    let control = unconfined(&interpreter, work.path(), &source).output().expect("control run");
    assert!(
        String::from_utf8_lossy(&control.stdout).contains("sk-not-for-the-child"),
        "the control must read the credential, or this case proves nothing: {control:?}"
    );

    let confined = confinement()
        .command(&launch(&interpreter, work.path(), &source, None))
        .expect("confined command")
        .stdin(Stdio::null())
        .output()
        .expect("confined run");

    assert!(
        !String::from_utf8_lossy(&confined.stdout).contains("sk-not-for-the-child"),
        "a confined launch read an unbound credential"
    );
    assert!(!confined.status.success(), "and it should have failed outright: {confined:?}");
}

/// An inherited provider key does not survive `--clearenv`: the confined child
/// starts with exactly the environment the launch declared. §AR-neural-admission.5
#[test]
fn a_confined_launch_starts_with_only_the_environment_it_was_given() {
    let interpreter = interpreter();
    let work = Scratch::new("environment");
    let source = "import os,sys\nsys.stdout.write(repr(sorted(os.environ)))\n";
    let mut confined = launch(&interpreter, work.path(), source, None);
    confined.environment.insert("RHEI_BROKER_TOKEN".into(), "granted".into());

    let output = confinement()
        .command(&confined)
        .expect("confined command")
        .env("OPENAI_API_KEY", "sk-inherited")
        .stdin(Stdio::null())
        .output()
        .expect("confined run");

    let seen = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{output:?}");
    assert!(!seen.contains("OPENAI_API_KEY"), "an inherited provider key survived: {seen}");
    assert!(
        seen.contains("RHEI_BROKER_TOKEN"),
        "the declared broker credential is missing: {seen}"
    );
}

/// The process tree is complete: a descendant that detaches itself is still in
/// the pid namespace whose init is the confined command, so ending that
/// command ends it too. §FS-rhei-run.3.2 §FS-rhei-budgets.6.4
#[test]
fn ending_a_confined_launch_ends_a_descendant_that_detached_itself() {
    let interpreter = interpreter();
    let work = Scratch::new("teardown");
    let marker = work.path().join("escaped");
    // The grandchild starts its own session, so nothing but the namespace
    // collapsing can stop it writing after its parent is gone.
    let source = "import subprocess,sys,time\n\
         subprocess.Popen([sys.executable,'-I','-c',\
         'import pathlib,sys,time;time.sleep(2.0);pathlib.Path(sys.argv[1]).write_text(\"escaped\")',\
         sys.argv[1]],start_new_session=True)\n\
         sys.stdout.write('spawned\\n')\n\
         sys.stdout.flush()\n\
         time.sleep(30)\n";

    // Control: the same descendant writes its marker when nothing ends it.
    let control_work = Scratch::new("teardown-control");
    let control_marker = control_work.path().join("escaped");
    let mut control = unconfined(&interpreter, control_work.path(), source)
        .arg(&control_marker)
        .stdout(Stdio::piped())
        .spawn()
        .expect("control run");
    wait_for(&control_marker, Duration::from_secs(20));
    let _ = control.kill();
    let _ = control.wait();
    assert!(
        control_marker.exists(),
        "the detached descendant must write its marker, or this case proves nothing"
    );

    let mut confined = launch(&interpreter, work.path(), source, None);
    confined.argv.push(marker.display().to_string());
    let mut child = confinement()
        .command(&confined)
        .expect("confined command")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .expect("confined run");
    let mut spawned = [0u8; 8];
    child.stdout.as_mut().expect("piped stdout").read_exact(&mut spawned).expect("read handshake");
    assert_eq!(&spawned, b"spawned\n");

    child.kill().expect("end the namespace leader");
    child.wait().expect("reap the namespace leader");

    // Well past the descendant's own delay: if the namespace had not ended it,
    // the marker would be there by now.
    std::thread::sleep(Duration::from_secs(4));
    assert!(!marker.exists(), "a detached descendant outlived its confined launch");
}

fn wait_for(path: &Path, limit: Duration) {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline && !path.exists() {
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The broker socket is the one thing that crosses the boundary, and it is
/// mounted at a fixed path so a qualified configuration can name it without
/// learning a host path. §FS-rhei-budgets.6.4
#[test]
fn the_broker_socket_is_the_only_path_out_of_the_namespace() {
    let interpreter = interpreter();
    let work = Scratch::new("broker");
    let socket_dir = Scratch::new("broker-socket");
    let socket = socket_dir.path().join("broker.sock");
    let listener =
        std::os::unix::net::UnixListener::bind(&socket).expect("host-side broker socket");
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let _ = stream.write_all(b"broker\n");
        }
    });
    let source = format!(
        "import socket,sys\n\
         s=socket.socket(socket.AF_UNIX);s.settimeout(3);s.connect({:?})\n\
         sys.stdout.write(s.recv(16).decode())\n",
        BROKER_MOUNT
    );

    let output = confinement()
        .command(&launch(&interpreter, work.path(), &source, Some(socket.clone())))
        .expect("confined command")
        .stdin(Stdio::null())
        .output()
        .expect("confined run");

    assert!(output.status.success(), "the brokered channel must work: {output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("broker"));

    // And without it there is no channel at all.
    let closed = confinement()
        .command(&launch(&interpreter, work.path(), &source, None))
        .expect("confined command")
        .stdin(Stdio::null())
        .output()
        .expect("confined run");
    assert!(!closed.status.success(), "a launch with no broker still had one: {closed:?}");
}

/// A relative bind would be resolved against whatever directory the engine
/// happened to be in, so the builder refuses it rather than confining the
/// wrong path. §AR-neural-admission.5
#[test]
fn a_confined_launch_refuses_a_path_it_cannot_pin() {
    let confinement = confinement();
    let mut relative = ConfinedLaunch {
        program: PathBuf::from("python3"),
        argv: vec![],
        read_only: vec![],
        work: PathBuf::from("/tmp"),
        broker_socket: None,
        environment: BTreeMap::new(),
    };
    assert_eq!(confinement.command(&relative).unwrap_err().reason_code, "missing_qualification");
    relative.program = PathBuf::from("/usr/bin/python3");
    relative.work = PathBuf::from("work");
    assert_eq!(confinement.command(&relative).unwrap_err().reason_code, "missing_qualification");
}
