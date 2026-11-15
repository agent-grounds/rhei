// The fixture helper `append` held to its rule: two processes that append to one
// file at the same instant both land their line §REQ-cross-platform.4.2.

// Only the e2e harness compiles this file. The integration harness includes
// `python_fixture.rs` too, so a test written there would run once per binary.

use std::process::{Command, Stdio};

use super::{python_command, unique_temp_dir, write_python_agent};

/// Windows' append for every append-mode open in a writer: seek to the end,
/// then write, held between the two until the other writer has sought.
const CRT_APPEND: &str = include_str!("crt_append.py");

/// The rendezvous between a writer's seek and its write. A writer that appends
/// under a lock waits it out once, while the other waits for the lock, so it is
/// what a green run of this test costs; long enough for two writers that left
/// the start together to reach it.
const RENDEZVOUS_SECONDS: &str = "2";

/// How long a writer waits for the other to start. A margin, not the behaviour
/// §REQ-cross-platform.8: a stalled interpreter must not leave the other writer
/// to time out at the rendezvous alone and append unraced.
const START_SECONDS: &str = "60";

/// One writer, after the prelude: append one line through the helper, the way
/// an arm of the flaky fan-out test does. `argv`: the target, the rendezvous
/// directory, the start directory, the writer's name, and the two bounds.
const WRITER: &str = r#"target, meet, start, name, rendezvous, start_bound = sys.argv[1:7]
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import crt_append
crt_append.install(meet, name, 2, float(rendezvous))
if not crt_append.arrive(start, name, 2, float(start_bound)):
    sys.exit('writer ' + name + ': the other writer did not start within ' + start_bound + ' s')
append(target, 'topic2-1 ' + name + ' together\n')
"#;

/// Two arms append to one log at once, with every append taking Windows' two
/// steps and each writer held between them until the other has sought. Both
/// lines land only when the helper serialises the append. §REQ-cross-platform.4.2
#[test]
#[ignore = "red until #723: under Windows' seek-then-write append the fixture helper `append` \
            keeps one of two concurrent lines; implement removes this"]
fn fixture_append_keeps_both_lines_two_processes_write_at_once() {
    let dir = unique_temp_dir("fixture-append");
    let meet = dir.join("meet");
    let start = dir.join("start");
    std::fs::create_dir_all(&meet).expect("create the rendezvous directory");
    std::fs::create_dir_all(&start).expect("create the start directory");
    std::fs::write(dir.join("crt_append.py"), CRT_APPEND).expect("write the emulation");
    let writer = write_python_agent(&dir, "writer.py", WRITER);
    let log = dir.join("arms.log");

    let writers: Vec<_> = ["alpha", "beta"]
        .into_iter()
        .map(|name| {
            let child = Command::new(python_command())
                .arg(&writer)
                .arg(&log)
                .arg(&meet)
                .arg(&start)
                .args([name, RENDEZVOUS_SECONDS, START_SECONDS])
                // The prelude reads an agent's prompt from stdin under an attempt.
                .env_remove("RHEI_ATTEMPT")
                .env("PYTHONDONTWRITEBYTECODE", "1")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("start a writer");
            (name, child)
        })
        .collect();
    for (name, child) in writers {
        let output = child.wait_with_output().expect("wait for a writer");
        assert!(
            output.status.success(),
            "writer {name} failed with {}:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let mut sought: Vec<String> = std::fs::read_dir(&meet)
        .expect("read the rendezvous directory")
        .map(|entry| entry.expect("rendezvous entry").file_name().to_string_lossy().into_owned())
        .collect();
    sought.sort_unstable();
    assert_eq!(
        sought,
        ["alpha", "beta"],
        "both appends should take the emulated seek-then-write; one that bypassed it proves \
         nothing about Windows"
    );

    let text = std::fs::read_to_string(&log).unwrap_or_default();
    let mut lines: Vec<&str> = text.lines().collect();
    lines.sort_unstable();
    assert_eq!(
        lines,
        ["topic2-1 alpha together", "topic2-1 beta together"],
        "two processes each appended one line through the fixture helper at the same instant, \
         under Windows' seek-then-write append; {} holds {text:?}",
        log.display()
    );
}
