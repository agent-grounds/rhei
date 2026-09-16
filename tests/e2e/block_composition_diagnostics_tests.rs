use std::fs;
use std::path::Path;

use super::block_composition_support::*;
use super::*;

fn direct_failure(dir: &Path, review: &Path, fix: &Path, extra: &[&str]) -> CliRun {
    let review = mount_arg("review", review);
    let fix = mount_arg("fix", fix);
    let mut args = vec!["instantiate", "--mount", review.as_str(), "--mount", fix.as_str()];
    args.extend_from_slice(extra);
    run_compose(dir, &args)
}

/// §FS-rhei-library.8 §FS-rhei-errors.1.6
#[test]
fn invalid_alias_diagnostic_names_the_refused_alias_and_grammar() {
    let dir = unique_temp_dir("blocks-diagnostics-alias");
    let fixtures = write_composition_fixtures(&dir);
    let bad_alias = mount_arg("bad.alias", &fixtures.review);
    let invalid = run_compose(&dir, &["instantiate", "--mount", bad_alias.as_str()]);
    assert_failed_with(&invalid, &["bad.alias", "alias", "letter"]);
}

/// §FS-rhei-library.8 §FS-rhei-errors.1.6
#[test]
fn duplicate_alias_diagnostic_names_both_manifests() {
    let dir = unique_temp_dir("blocks-diagnostics-duplicate-alias");
    let fixtures = write_composition_fixtures(&dir);
    let first = mount_arg("same", &fixtures.review);
    let second = mount_arg("same", &fixtures.fix);
    let duplicate =
        run_compose(&dir, &["instantiate", "--mount", first.as_str(), "--mount", second.as_str()]);
    assert_failed_with(
        &duplicate,
        &[
            "same",
            "duplicate",
            fixtures.review.to_str().expect("review path"),
            fixtures.fix.to_str().expect("fix path"),
        ],
    );
}

/// §FS-rhei-library.8 §FS-rhei-errors.1.3
#[test]
fn unknown_block_diagnostic_keeps_the_mount_alias_and_next_action() {
    let dir = unique_temp_dir("blocks-diagnostics-unknown-block");
    let absent = run_compose(&dir, &["instantiate", "--mount", "work=no-such-block"]);
    assert_failed_with(&absent, &["no-such-block", "work", "template"]);
}

/// §FS-rhei-library.1–2 §FS-rhei-library.8
#[test]
fn unknown_control_port_diagnostic_lists_public_alternatives() {
    let dir = unique_temp_dir("blocks-diagnostics-endpoint");
    let fixtures = write_composition_fixtures(&dir);
    let port = direct_failure(
        &dir,
        &fixtures.review,
        &fixtures.fix,
        &["--seam", "review.internal=fix.entry"],
    );
    assert_failed_with(&port, &["review.internal", "done", "cancelled", "review-block"]);
}

/// §FS-rhei-library.1–2 §FS-rhei-library.8
#[test]
fn unknown_data_endpoint_diagnostic_lists_public_alternatives() {
    let dir = unique_temp_dir("blocks-diagnostics-data-endpoint");
    let fixtures = write_composition_fixtures(&dir);
    let endpoint = direct_failure(
        &dir,
        &fixtures.review,
        &fixtures.fix,
        &["--seam", "review.done=fix.entry", "--pass", "review.unknown=fix.report"],
    );
    assert_failed_with(&endpoint, &["review.unknown", "report", "findings", "review-block"]);
}

/// §FS-rhei-library.6 §FS-rhei-library.8
#[test]
fn data_kind_mismatch_diagnostic_names_both_manifests_and_kinds() {
    let dir = unique_temp_dir("blocks-diagnostics-kind");
    let fixtures = write_composition_fixtures(&dir);
    let kind = direct_failure(
        &dir,
        &fixtures.review,
        &fixtures.fix,
        &["--seam", "review.done=fix.entry", "--pass", "review.report=fix.findings"],
    );
    assert_failed_with(
        &kind,
        &[
            "review.report",
            "fix.findings",
            "state-file",
            "task-export",
            fixtures.review.to_str().expect("review path"),
            fixtures.fix.to_str().expect("fix path"),
        ],
    );
}

fn write_cycle_block(root: &Path, name: &str, child: &str, alias: &str) -> std::path::PathBuf {
    let block = root.join(name);
    fs::create_dir_all(&block).expect("cycle block");
    write_fixture_file(
        &block,
        "template.yaml",
        &format!(
            "name: {name}\nversion: 1\ndescription: cycle fixture\nports:\n  entry: {alias}.entry\n  exits:\n    done: {alias}.done\nuse:\n  - {{ block: ../{child}, as: {alias} }}\n"
        ),
    );
    block
}

/// Recursive expansion distinguishes a stack cycle from legal repeated use
/// and prints the complete manifest/alias chain. §FS-rhei-library.2
#[test]
fn recursive_cycle_diagnostic_prints_the_whole_chain() {
    let dir = unique_temp_dir("blocks-diagnostics-cycle");
    let a = write_cycle_block(&dir, "cycle-a", "cycle-b", "b");
    let b = write_cycle_block(&dir, "cycle-b", "cycle-a", "a");
    let result = run_compose(&dir, &["instantiate", a.to_str().expect("cycle root")]);
    assert_failed_with(
        &result,
        &[
            "cycle",
            "cycle-a",
            "cycle-b",
            a.join("template.yaml").to_str().expect("a manifest"),
            b.join("template.yaml").to_str().expect("b manifest"),
            "b",
            "a",
        ],
    );
}

/// §FS-rhei-library.2 §FS-rhei-library.8
#[test]
fn bad_bind_diagnostic_names_both_manifests_and_the_valid_input() {
    let dir = unique_temp_dir("blocks-diagnostics-bind");
    let fixtures = write_composition_fixtures(&dir);
    let flow_manifest = fixtures.flow.join("template.yaml");
    let original = fs::read_to_string(&flow_manifest).expect("flow manifest");
    fs::write(&flow_manifest, original.replace("to: review.subject", "to: review.missing"))
        .expect("bad bind fixture");
    let bind = run_compose(&dir, &["instantiate", fixtures.flow.to_str().expect("flow"), "auth"]);
    assert_failed_with(
        &bind,
        &[
            "review.missing",
            "review.subject",
            flow_manifest.to_str().expect("flow manifest"),
            fixtures.review.join("template.yaml").to_str().expect("review manifest"),
        ],
    );
}

/// §FS-rhei-library.4 §FS-rhei-library.8
#[test]
fn dangling_owned_reference_diagnostic_keeps_its_mount_and_source() {
    let dir = unique_temp_dir("blocks-diagnostics-owned-reference");
    let fixtures = write_composition_fixtures(&dir);
    let states_path = fixtures.review.join("states.yaml");
    let states = fs::read_to_string(&states_path).expect("review states");
    fs::write(&states_path, states.replace("to: done", "to: absent-state"))
        .expect("dangling state fixture");
    let ownership = direct_failure(&dir, &fixtures.review, &fixtures.fix, &[]);
    assert_failed_with(
        &ownership,
        &["absent-state", "review", states_path.to_str().expect("states path"), "state"],
    );
}

/// §FS-rhei-library.7–8
#[test]
fn compatibility_collision_diagnostic_names_both_stable_identities() {
    let dir = unique_temp_dir("blocks-diagnostics-identity-map");
    let fixtures = write_composition_fixtures(&dir);
    let flow_manifest = fixtures.flow.join("template.yaml");
    let original = fs::read_to_string(&flow_manifest).expect("flow manifest");
    fs::write(
        &flow_manifest,
        format!(
            "{original}compatibility:\n  states:\n    stable-a: review.review\n    stable-b: review.review\n"
        ),
    )
    .expect("identity collision fixture");
    let identity =
        run_compose(&dir, &["instantiate", fixtures.flow.to_str().expect("flow"), "auth"]);
    assert_failed_with(
        &identity,
        &[
            "stable-a",
            "stable-b",
            "review.review",
            "collision",
            flow_manifest.to_str().expect("flow manifest"),
        ],
    );
}

/// An explicit seam set is total; a partial set cannot silently acquire the
/// missing default edge. §FS-rhei-library.2
#[test]
fn explicit_seams_must_form_one_complete_chain() {
    let dir = unique_temp_dir("blocks-diagnostics-chain");
    let fixtures = write_composition_fixtures(&dir);
    let review = mount_arg("review", &fixtures.review);
    let fix = mount_arg("fix", &fixtures.fix);
    let again = mount_arg("again", &fixtures.review);
    let result = run_compose(
        &dir,
        &[
            "instantiate",
            "--mount",
            review.as_str(),
            "--mount",
            fix.as_str(),
            "--mount",
            again.as_str(),
            "--seam",
            "review.done=fix.entry",
        ],
    );
    assert_failed_with(&result, &["complete", "again", "seam", "review", "fix"]);
}
