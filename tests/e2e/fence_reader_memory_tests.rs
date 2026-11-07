//! Public memory lookup must skip quoted legacy result markers.
//! §FS-rhei-plan-language.3.8 §FS-rhei-memory.4.3

use std::fs;

use super::fence_reader_support::*;
use super::*;

fn history(quotation: &str, qualified: bool) {
    let tasks = format!("### Task 1: Old finished task\n**State:** done\n\n{quotation}\n> **Result:** [1](runtime/results/1.md)\n\n### Task 2: Next\n**State:** open\n**Prior:** 1\n");
    let (dir, _) = fixture("rhei-614-memory", false, &tasks);
    fs::create_dir_all(dir.join("runtime/results")).expect("results");
    write_fixture_file(&dir, "runtime/results/1.md", "## Result\n\nREAL LEGACY ACCOUNT\n");
    write_fixture_file(&dir, "runtime/results/quoted.md", "## Result\n\nQUOTED DECOY ACCOUNT\n");
    if qualified {
        write_fixture_file(&dir, "runtime/results/plan.1.md", "## Result\n\nQUALIFIED ACCOUNT\n");
    }
    assert_success(&run(&dir, &["validate", "plan.rhei.md"]));
    let next =
        run(&dir, &["next", "plan.rhei.md", "--task", "2", "--peek", "--no-callbacks", "--json"]);
    assert_success(&next);
    let value: serde_json::Value = serde_json::from_str(&next.stdout).expect("next JSON");
    let history = value["plan_history"].as_str().expect("history section");
    let expected = if qualified { "QUALIFIED ACCOUNT" } else { "REAL LEGACY ACCOUNT" };
    assert!(
        history.contains(expected),
        "history must read {expected} after {quotation:?}; got:\n{history}"
    );
    assert!(!history.contains("QUOTED DECOY ACCOUNT"), "quoted marker became a result:\n{history}");
    if qualified {
        assert!(
            !history.contains("REAL LEGACY ACCOUNT"),
            "qualified precedence changed:\n{history}"
        );
    }
}

const QUOTED_TILDE: &str = "~~~markdown\n> **Result:** [quoted](runtime/results/quoted.md)\n~~~\n";
const QUOTED_LONGER: &str =
    "````markdown\n```console\n> **Result:** [quoted](runtime/results/quoted.md)\n````\n";
const QUOTED_INFO: &str =
    "```markdown\n```console\n> **Result:** [quoted](runtime/results/quoted.md)\n```\n";

macro_rules! red_memory {
    ($name:ident, $quote:expr) => {
        #[test]
        fn $name() {
            history($quote, false);
        }
    };
}

red_memory!(fence_reader_614_memory_tilde, QUOTED_TILDE);
red_memory!(fence_reader_614_memory_longer, QUOTED_LONGER);
red_memory!(fence_reader_614_memory_info, QUOTED_INFO);

/// §FS-rhei-plan-language.3.8: quoted links cannot change qualified precedence.
#[test]
fn fence_reader_614_memory_qualified_control() {
    for quote in [QUOTED_TILDE, QUOTED_LONGER, QUOTED_INFO] {
        history(quote, true);
    }
}

#[test]
fn fence_reader_614_memory_ordinary_control() {
    history("```markdown\n> **Result:** [quoted](runtime/results/quoted.md)\n```\n", false);
}
