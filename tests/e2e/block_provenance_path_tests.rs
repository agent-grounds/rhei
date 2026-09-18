//! Source lookup preserves authored paths and every winning diagnostic locator.
//! §FS-rhei-library.4.1
use std::fs;

use super::block_composition_support::*;
use super::block_provenance_source_tests::instantiate_one;
use super::block_provenance_support::*;
use super::*;

#[test]
fn single_file_and_workspace_tasks_identify_their_authored_files() {
    let dir = provenance_test_dir("task-paths");
    for (name, single, expected) in
        [("single", true, "plan.rhei.md"), ("workspace", false, "tasks/work.md")]
    {
        let source = dir.join(name);
        write_provenance_block(&source, single);
        let output = instantiate_one(&dir, source.to_str().unwrap(), &format!("out-{name}"));
        let lock = read_lock(&output);
        let origin = &lock["nodes"]["tasks"]["m6_sample__job"][0];
        let declaration = &lock["declarations"][origin["declaration"].as_str().unwrap()];
        assert_eq!(declaration["file"], expected);
        assert!(source.join(expected).is_file());
        assert_eq!(object_keys(&lock["nodes"]["tasks"]), task_ids(&output));
        assert_complete_origins(&lock);
    }
}

#[test]
fn identical_absolute_sources_reconcile_locators_without_losing_mounts() {
    let dir = provenance_test_dir("locators");
    let a = dir.join("a");
    let b = dir.join("copy/a");
    write_provenance_block(&a, false);
    write_provenance_block(&b, false);
    let mounts = [mount_arg("a", &a), mount_arg("b", &b)];
    let mut locks = Vec::new();
    for (name, order) in [("forward", [0, 1]), ("reverse", [1, 0])] {
        let output = dir.join(name);
        let result = rhei_command(dir.join(".home"))
            .current_dir(&dir)
            .env("GIT_CEILING_DIRECTORIES", fs::canonicalize(&dir).unwrap())
            .args([
                "instantiate",
                "--mount",
                &mounts[order[0]],
                "--mount",
                &mounts[order[1]],
                "--output",
                output.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert_success(&CliRun::from(&result));
        locks.push(read_lock(&output));
    }
    assert_eq!(
        locks[0]["sources"], locks[1]["sources"],
        "source reconciliation ignores mount traversal order"
    );
    assert_eq!(locks[0]["mounts"], locks[1]["mounts"]);
    let lock = &locks[0];
    let source_id = lock["mounts"]["m1_a__"]["source"].as_str().unwrap();
    assert_eq!(lock["mounts"]["m1_b__"]["source"], source_id);
    let source = &lock["sources"][source_id];
    let locators = source["locators"].as_array().unwrap();
    assert_eq!(locators.len(), 2);
    assert_eq!(source["locator"], locators[0]);
    assert!(
        locators[0]["requested"].as_str().unwrap() < locators[1]["requested"].as_str().unwrap()
    );
    for (mount, path) in [("m1_a__", a), ("m1_b__", b)] {
        let locator = &lock["mounts"][mount]["locator"];
        assert_eq!(locator["requested"], path.to_str().unwrap().replace('\\', "/"));
        assert_eq!(locator["portable"], false);
        assert!(locators.contains(locator));
    }
    assert_eq!(
        lock["nodes"]["states"]["m1_a__work"][0]["declaration"],
        lock["nodes"]["states"]["m1_b__work"][0]["declaration"]
    );
    assert_complete_origins(lock);
}
