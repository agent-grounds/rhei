//! Attended basin recovery uses the project manifest and basin ledger. §FS-rhei-recover.2

use super::operator_attended_tests::attended;
use super::operator_force_support::ForceFixture;
use super::*;

/// Real CLI success preserves qualified counters, checkpoint delivery and fresh result history. §FS-rhei-transition-cmd.6.1
#[test]
fn operator_basin_attended_final_entry_preserves_manifest_and_siblings() {
    let dir = unique_temp_dir("operator-basin-attended");
    let manifest = "# Panta: Recovery\n---\nowner: keep-me\nmetadata:\n  tasks:\n    basin.9:\n      stateVisits:\n        work: 2\n---\n\nProject prose stays byte-for-byte.\n";
    let plan = write_fixture_file(&dir, "index.panta.md", manifest);
    let machine = write_fixture_file(&dir, "states.yaml", "name: recovery\nversion: 1\nstates:\n  gate:\n    initial: true\n    gating: true\n  work:\n    visits: 3\n  supervising:\n    agent: pi\n    execute_on: descendant-terminal\n    visits: 3\n  done:\n    final: true\n  cancelled:\n    final: true\ntransitions:\n  - {from: gate, to: done}\n  - {from: work, to: cancelled}\n  - {from: supervising, to: supervising}\n  - {from: supervising, to: done}\n");
    let task = write_fixture_file(
        &dir,
        "basin/work.md",
        "### Task 1: Parent\n**State:** supervising\n\n#### Task 1.1: Fix\n**State:** work\n",
    );
    let sibling =
        write_fixture_file(&dir, "basin/sibling.md", "### Task 9: Sibling\n**State:** gate\n");
    let other = write_fixture_file(
        &dir,
        "other.rhei.md",
        "# Rhei: Other\n\n## Tasks\n\n### Task 1: Independent\n**State:** gate\n",
    );
    let sibling_bytes = fs::read(&sibling).unwrap();
    let other_bytes = fs::read(&other).unwrap();
    assert_success(&run_cli("validate", &plan, &machine, &[]));
    let result =
        write_fixture_file(&dir, "basin/runtime/results/basin.1.1.md", "Earlier result history.\n");
    let before = [&plan, &task, &result].map(|path| fs::read(path).unwrap());
    let refused = run_cli(
        "transition",
        &plan,
        &machine,
        &[
            "--task",
            "basin.1.1",
            "--from",
            "work",
            "--to",
            "done",
            "--force",
            "--reason",
            "repair basin",
        ],
    );
    assert!(!refused.status.success());
    assert!(refused.stderr.contains("requires a fresh non-empty --result"), "{}", refused.stderr);
    for (path, bytes) in [&plan, &task, &result].into_iter().zip(before) {
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
    let fixture = ForceFixture { dir, plan, machine };
    let (success, transcript) = attended(
        &fixture,
        &[
            "--state-machine",
            fixture.machine.to_str().unwrap(),
            "transition",
            fixture.plan.to_str().unwrap(),
            "--task",
            "basin.1.1",
            "--from",
            "work",
            "--to",
            "done",
            "--force",
            "--reason",
            "repair basin",
            "--result",
            "Fresh basin outcome.",
        ],
        "force basin.1.1 work -> done\r\n",
    );
    assert!(success, "{transcript}");
    let after = fs::read_to_string(&fixture.plan).unwrap();
    assert!(after.contains("owner: keep-me\n"));
    assert!(after.ends_with("Project prose stays byte-for-byte.\n"));
    let parsed = rhei_core::parser::parse_panta_manifest(&after).unwrap();
    let yaml = serde_json::to_value(parsed.metadata).unwrap();
    assert_eq!(yaml["metadata"]["tasks"]["basin.9"]["stateVisits"]["work"].as_u64(), Some(2));
    assert_eq!(yaml["metadata"]["tasks"]["basin.1.1"]["stateVisits"]["work"].as_u64(), Some(1));
    assert!(!yaml["metadata"]["tasks"]["basin.1"].is_null());
    assert_eq!(after.matches("checkpoints:").count(), 1, "{after}");
    assert_eq!(
        fs::read_to_string(&result).unwrap(),
        "Earlier result history.\n## Result\n\nFresh basin outcome.\n\n"
    );
    let task_after = fs::read_to_string(&task).unwrap();
    assert!(task_after.contains("**State:** done"));
    assert_eq!(task_after.matches("> **Result:**").count(), 1);
    assert_eq!(fs::read(&sibling).unwrap(), sibling_bytes);
    assert_eq!(fs::read(&other).unwrap(), other_bytes);
    let ledger =
        fs::read_to_string(fixture.dir.join("basin/runtime/state-transitions.log")).unwrap();
    let moves = rhei_core::transition_history::parse(&ledger).unwrap();
    assert_eq!(moves.len(), 1);
    assert_eq!(moves[0].task_id, "basin.1.1");
    assert_eq!(moves[0].audit.as_ref().unwrap().reason, "repair basin");
    assert!(!fixture.dir.join("runtime/state-transitions.log").exists());
    assert!(!fixture.dir.join("basin/runtime/transitions.log").exists());
    assert!(!fixture.dir.join("basin/.rhei/forced-recovery.json").exists());
    assert!(!fixture.dir.join(".rhei/forced-recovery.json").exists());
}
