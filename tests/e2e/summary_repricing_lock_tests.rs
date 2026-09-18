//! Contended ownership must be trustworthy before a summary can reprice a run.
//! §FS-rhei-summary.1 §FS-rhei-summary.5

use std::io::{Read, Seek, Write};

use super::run_cli;
use super::summary_repricing_support::*;

#[test]
fn held_invalid_ownership_cannot_authorize_a_priced_summary() {
    for case in ["unsupported", "malformed", "legacy", "foreign-workspace"] {
        let fixture = RepriceFixture::new(&format!("summary-reprice-owner-{case}"));
        fixture.seed_changed_rate_run();
        let mut lock = hold_run_lock(&fixture.root, OTHER_RUN);
        lock.rewind().expect("rewind owner");
        let mut owner: serde_json::Value = serde_json::from_reader(&lock).expect("v1 owner");
        match case {
            "unsupported" => owner["version"] = 99.into(),
            "malformed" => owner["pid"] = "invalid".into(),
            "legacy" => owner = serde_json::json!({"id": OTHER_RUN}),
            "foreign-workspace" => {
                owner["workspace"] = serde_json::json!(fixture.root.join("runtime"))
            }
            _ => unreachable!(),
        }
        let before = serde_json::to_vec(&owner).expect("serialize invalid owner");
        lock.rewind().expect("rewind owner");
        lock.set_len(0).expect("clear owner");
        lock.write_all(&before).expect("write invalid owner");
        lock.flush().expect("flush invalid owner");
        let runtime_before = tree_snapshot(&fixture.root.join("runtime"));

        let book = fixture.later_book.to_string_lossy();
        let result = run_cli(
            "summary",
            &fixture.plan,
            &fixture.machine,
            &["--run", SELECTED_RUN, "--prices", &book],
        );
        let diagnostic = format!("{case}: stdout:\n{}\nstderr:\n{}", result.stdout, result.stderr);
        assert!(!result.status.success(), "{diagnostic}");
        assert!(result.stdout.is_empty(), "{diagnostic}");
        assert!(result.stderr.contains("activity cannot be determined"), "{diagnostic}");
        assert!(result.stderr.contains("run.lock"), "{diagnostic}");
        assert_eq!(tree_snapshot(&fixture.root.join("runtime")), runtime_before);
        lock.rewind().expect("rewind inspected owner");
        let mut after = Vec::new();
        lock.read_to_end(&mut after).expect("read inspected owner");
        assert_eq!(after, before, "{case}: summary changed ownership");
    }
}
