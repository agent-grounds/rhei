//! Reset must clear basin runtime metadata in its owning manifest and preserve
//! everything outside the register's cleared keys and selected scope.
//! §FS-rhei-reset.2 §FS-rhei-reset.2.1 §FS-rhei-transitions.2.5

use super::reset_basin_support::*;

/// Port of intake's engine-counted scenario, with a workspace control and
/// snapshots before re-entry so a new counter cannot mask a leftover one.
fn counted_reset(narrow: bool) {
    let world = project("reset-basin-counted");
    world.enter("basin.1", "work");
    world.enter("ws.1", "work");
    let before = frontmatter(&world, "index.panta.md");
    assert_eq!(before["metadata"]["tasks"]["basin.1"]["stateVisits"]["work"], 1);
    let identity = before["metadata"]["tasks"]["basin.1"]["budgetTicketId"].clone();
    assert!(identity.as_str().is_some(), "fixture: the engine writes a budget identity");
    let workspace_index = read(&world, "ws/index.rhei.md");
    assert!(workspace_index.contains("stateVisits:"), "fixture: workspace count was written");
    let workspace_task = read(&world, "ws/tasks/01-first.md");
    world.write("runtime/results/basin.1.md", "basin result\n");
    world.write("runtime/results/ws.1.md", "workspace result\n");
    world.write("ws/runtime/results/ws.1.md", "member result\n");
    world.step("validate", &[]);

    reset(&world, narrow);
    assert_eq!(world.state_of("basin.1"), "open", "reset restored the authored state");
    let after = frontmatter(&world, "index.panta.md");
    assert_eq!(after["metadata"]["tasks"]["basin.1"]["budgetTicketId"], identity);
    assert!(!world.root.join("runtime/results/basin.1.md").exists());
    if narrow {
        assert_eq!(read(&world, "ws/index.rhei.md"), workspace_index);
        assert_eq!(read(&world, "ws/tasks/01-first.md"), workspace_task);
        assert_eq!(read(&world, "runtime/results/ws.1.md"), "workspace result\n");
        assert_eq!(read(&world, "ws/runtime/results/ws.1.md"), "member result\n");
    } else {
        assert!(!world.root.join("runtime").exists());
        assert!(!world.root.join("ws/runtime").exists());
        assert!(!read(&world, "ws/index.rhei.md").contains("stateVisits:"));
        world.enter("ws.1", "work");
    }
    assert!(read(&world, "ws/tasks/01-first.md").starts_with(AUTHORED_BLOCK));

    let again = world.rhei(
        "transition",
        &["--task", "basin.1", "--from", "open", "--to", "work", "--no-callbacks"],
    );
    assert!(
        again.status.success(),
        "basin.1 must re-enter work after successful reset (narrow={narrow}); \
         transition exit {:?}; metadata immediately after reset: {after:?}\n{}{}",
        again.status.code(),
        again.stdout,
        again.stderr
    );
    assert!(after["metadata"]["tasks"]["basin.1"]["stateVisits"].is_null());
    assert_eq!(
        frontmatter(&world, "index.panta.md")["metadata"]["tasks"]["basin.1"]["stateVisits"]
            ["work"],
        1,
        "re-entry starts at visit one"
    );
}

#[test]
fn project_reset_restarts_basin_counted_visits() {
    counted_reset(false);
}

#[test]
fn narrowed_reset_restarts_basin_counted_visits() {
    counted_reset(true);
}

fn metadata_reset(narrow: bool) {
    let world = project("reset-basin-metadata");
    let expected = seed_metadata(&world);
    world.step("validate", &[]);
    reset(&world, narrow);
    let listing = world.rhei("list", &[]);
    assert!(listing.status.success());
    for (id, title) in [("basin.1", "First"), ("basin.1.1", "Child"), ("basin.2", "Never moved")] {
        let expected = format!("Task {id}: {title} [open]");
        assert!(listing.stdout.lines().any(|line| line.trim() == expected), "{}", listing.stdout);
    }
    world.step("validate", &[]);
    assert_eq!(
        frontmatter(&world, "index.panta.md")["metadata"],
        expected,
        "reset must clear stateVisits, providerLimits and supervision even for \
         never-moved tickets and descendants, pruning only emptied maps"
    );
    assert!(read(&world, "index.panta.md").ends_with("Project prose.\n"));
}

#[test]
fn project_reset_clears_basin_runtime_metadata_only() {
    metadata_reset(false);
}

#[test]
fn narrowed_reset_clears_basin_runtime_metadata_only() {
    metadata_reset(true);
}

/// When no preserved data remains, prune the emptied ancestors too.
/// §FS-rhei-reset.2
fn empty_metadata_reset(narrow: bool) {
    let world = project("reset-basin-empty");
    let mut metadata: serde_yaml::Value =
        serde_yaml::from_str("tasks:\n  basin.1: {}\n").expect("metadata");
    metadata["tasks"]["basin.1"] = runtime_keys();
    write_manifest(&world, &metadata);
    world.step("validate", &[]);
    reset(&world, narrow);
    assert!(
        frontmatter(&world, "index.panta.md")["metadata"].is_null(),
        "reset prunes emptied metadata (narrow={narrow}): {}",
        read(&world, "index.panta.md")
    );
}

#[test]
fn project_reset_prunes_empty_basin_metadata_containers() {
    empty_metadata_reset(false);
}

#[test]
fn narrowed_reset_prunes_empty_basin_metadata_containers() {
    empty_metadata_reset(true);
}

/// Control and inverse scope boundary: selecting the workspace must leave the
/// shared basin manifest and basin artifacts byte-identical. §FS-rhei-reset.2.1
#[test]
fn workspace_reset_keeps_out_of_scope_basin_metadata() {
    let world = project("reset-basin-scope-control");
    seed_metadata(&world);
    world.enter("ws.1", "work");
    assert!(read(&world, "ws/index.rhei.md").contains("stateVisits:"));
    let manifest = read(&world, "index.panta.md");
    world.write("runtime/results/basin.1.md", "basin result\n");
    world.step("reset", &["--rhei", "ws", "--yes"]);
    assert_eq!(read(&world, "index.panta.md"), manifest);
    assert_eq!(read(&world, "runtime/results/basin.1.md"), "basin result\n");
    assert_eq!(world.state_of("ws.1"), "open");
    assert!(!read(&world, "ws/index.rhei.md").contains("stateVisits:"));
    assert!(read(&world, "ws/tasks/01-first.md").starts_with(AUTHORED_BLOCK));
    world.enter("ws.1", "work");
}
