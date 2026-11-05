//! The shared completion writer reports failures for agents/programs and serial/parallel runs.
//! §FS-rhei-agents.8.4 §FS-rhei-agents.8.1 §FS-rhei-run.3

use super::spawn_record_failure_support::{assert_refusal_halt, Fixture};
use super::*;

fn completed_failure(agent: bool, parallel: &'static str, fault: &str) {
    let fixture = Fixture::new(agent, parallel, fault, 0, false);
    let out = fixture.run();
    fixture.assert_worker(&out, 0, true);
    fixture.assert_warning(&out, &fixture.cause(fault));
}

fn refusal_after_failure(agent: bool, parallel: &'static str) {
    let fixture = Fixture::new(agent, parallel, "directory", 1, false);
    let first = fixture.run();
    fixture.assert_worker(&first, 1, false);
    let before = fs::read(fixture.log()).unwrap();
    fixture.clear_fault("directory");
    let second = fixture.run();
    fixture.assert_worker(&second, 1, false);
    assert_eq!(fs::read(fixture.log()).unwrap(), before, "refusal preserves every log byte");
    assert_refusal_halt(&second);
}

macro_rules! dispatch_contract {
    ($directory:ident, $publication:ident, $refusal:ident, $agent:expr, $parallel:expr) => {
        #[test]
        #[ignore = "red until #548: report completion-record directory errors; implement removes this"]
        fn $directory() {
            completed_failure($agent, $parallel, "directory");
        }

        #[test]
        #[ignore = "red until #548: report shared record transaction errors; implement removes this"]
        fn $publication() {
            completed_failure($agent, $parallel, "read");
        }

        #[test]
        #[ignore = "red until #548: identify refused spawning in halt help; implement removes this"]
        fn $refusal() {
            refusal_after_failure($agent, $parallel);
        }
    };
}

dispatch_contract!(
    program_serial_directory,
    program_serial_publication,
    program_serial_refusal,
    false,
    "1"
);
dispatch_contract!(
    program_parallel_directory,
    program_parallel_publication,
    program_parallel_refusal,
    false,
    "2"
);
dispatch_contract!(
    agent_serial_directory,
    agent_serial_publication,
    agent_serial_refusal,
    true,
    "1"
);
dispatch_contract!(
    agent_parallel_directory,
    agent_parallel_publication,
    agent_parallel_refusal,
    true,
    "2"
);

#[test]
#[ignore = "red until #548: report completion-record lock failures; implement removes this"]
fn completion_record_lock_failure_is_observable() {
    completed_failure(false, "1", "lock");
}
