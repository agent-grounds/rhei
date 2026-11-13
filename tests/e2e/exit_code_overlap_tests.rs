//! §FS-rhei-programs.8.2: a program state with two `exit_code` rules matching the same code and no
//! condition to tell them apart is refused when the machine loads, by `rhei states` and by
//! `rhei validate`, so declaration order never decides where the exit goes
//! (§FS-rhei-programs.3.2). The machine is the one from agent-grounds/rhei#663.

use super::byte_order_mark_tests::run_in;
use super::*;

/// The issue's machine, with the two exit-12 rules in the order given.
fn machine(first: &str, second: &str) -> String {
    format!(
        "name: exit-code-overlap
version: 1
states:
  analyze:
    initial: true
    program:
      command: [\"sh\", \"-c\", \"exit 12\"]
    program_timeout: 60s
  native: {{ gating: true }}
  human-intervention: {{ gating: true }}
  done: {{ final: true }}
transitions:
  - {{ from: analyze, to: {first}, exit_code: 12 }}
  - {{ from: analyze, to: {second}, exit_code: 12 }}
  - {{ from: native, to: done }}
  - {{ from: human-intervention, to: done }}
"
    )
}

const PLAN: &str = "# Rhei: Overlap\n\n## Tasks\n\n### Task 1: Analyze\n**State:** analyze\n";

#[test]
#[ignore = "red until #663: rhei states and rhei validate accept two exit-12 rules; implement removes this"]
fn states_and_validate_refuse_two_exit_code_rules_for_one_code() {
    for (first, second) in [("human-intervention", "native"), ("native", "human-intervention")] {
        let dir = unique_temp_dir("exit-code-overlap");
        write_fixture_file(&dir, "states.yaml", &machine(first, second));
        write_fixture_file(&dir, "plan.rhei.md", PLAN);
        for (command, args) in [
            ("states", &["states", "--state-machine", "states.yaml"][..]),
            ("validate", &["validate", "plan.rhei.md", "--state-machine", "states.yaml"][..]),
        ] {
            let run = run_in(&dir, args);
            assert_eq!(
                run.status.code(),
                Some(1),
                "`rhei {command}` must refuse the exit-12 rules to {first} and {second}\nstdout:\n{}\nstderr:\n{}",
                run.stdout,
                run.stderr
            );
            for needle in [
                "'analyze'",
                "12",
                &format!("'analyze' -> '{first}'"),
                &format!("'analyze' -> '{second}'"),
            ] {
                assert!(
                    run.stderr.contains(needle),
                    "`rhei {command}` should name {needle:?}; stderr:\n{}",
                    run.stderr
                );
            }
        }
    }
}
