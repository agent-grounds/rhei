use std::fs;
use std::path::Path;

use super::block_exposure_support::*;
use super::*;

fn replace_manifest(path: &Path, from: &str, to: &str) {
    let manifest = fs::read_to_string(path).expect("read manifest");
    assert!(manifest.contains(from), "fixture manifest is missing {from:?}");
    fs::write(path, manifest.replace(from, to)).expect("write changed manifest");
}

fn record_failure(
    failures: &mut Vec<String>,
    label: &str,
    result: &CliRun,
    output: &Path,
    fragments: &[&str],
) {
    let combined = format!("{}\n{}", result.stdout, result.stderr);
    if result.status.success() {
        failures.push(format!("{label}: command succeeded; output:\n{combined}"));
    }
    for fragment in fragments {
        if !combined.contains(fragment) {
            failures.push(format!("{label}: missing {fragment:?}; output:\n{combined}"));
        }
    }
    if output.exists() {
        failures.push(format!("{label}: published partial output at {}", output.display()));
    }
}

fn direct_invalid_case(
    root: &Path,
    label: &str,
    from: &str,
    to: &str,
    fragments: &[&str],
    failures: &mut Vec<String>,
) {
    let case = root.join(label);
    fs::create_dir_all(&case).expect("diagnostic case directory");
    let fixture = write_exposure_fixture(&case);
    replace_manifest(&fixture.leaf.join("template.yaml"), from, to);
    let output = case.join("output");
    let result = instantiate_direct(&case, &fixture.leaf, &output);
    record_failure(failures, label, &result, &output, fragments);
}

/// Exposure schema and resolution errors are diagnosed specifically rather
/// than accepted, ignored, or reported as a generic unknown `expose` field.
/// §FS-rhei-library.1.2, §FS-rhei-library.8
#[test]
fn invalid_exposure_declarations_report_the_kind_target_and_correction() {
    let dir = unique_temp_dir("blocks-expose-invalid-declarations");
    let mut failures = Vec::new();
    direct_invalid_case(
        &dir,
        "unknown-field",
        "ready: { local: internal-ready }",
        "ready: { local: internal-ready, surprise: true }",
        &["surprise", "expose.states.ready", "local"],
        &mut failures,
    );
    direct_invalid_case(
        &dir,
        "unresolved-local",
        "ready: { local: internal-ready }",
        "ready: { local: absent-state }",
        &["absent-state", "state", "local", "template.yaml"],
        &mut failures,
    );
    direct_invalid_case(
        &dir,
        "duplicate-target",
        "ready: { local: internal-ready }",
        "ready: { local: internal-ready }\n    also-ready: { local: internal-ready }",
        &["ready", "also-ready", "internal-ready", "claim"],
        &mut failures,
    );
    direct_invalid_case(
        &dir,
        "generated-collision",
        "ready: { local: internal-ready }",
        "internal-done: { local: internal-ready }",
        &["internal-done", "collision", "state", "template.yaml"],
        &mut failures,
    );
    direct_invalid_case(
        &dir,
        "ambiguous-target",
        "ready: { local: internal-ready }",
        "ready: { local: internal-ready, mount: child, name: ready }",
        &["ready", "local", "mount", "exactly one"],
        &mut failures,
    );
    direct_invalid_case(
        &dir,
        "unknown-registry",
        "    skills:\n      checklist: { local: internal-skill }",
        "    secrets:\n      checklist: { local: internal-skill }",
        &["secrets", "settings", "agents", "skills"],
        &mut failures,
    );
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

fn strip_other_public_references(wrapper: &Path) {
    let task_path = wrapper.join("tasks/01-observe.md");
    let task = fs::read_to_string(&task_path).expect("observer task");
    fs::write(&task_path, task.replace("**Prior:** Task review.audit\n", ""))
        .expect("strip public prior");
    let machine_path = wrapper.join("states.yaml");
    let machine = fs::read_to_string(&machine_path).expect("observer machine");
    let machine = machine
        .replace("    agent: review.reviewer\n", "")
        .replace("    model: review.careful\n", "")
        .replace("    mcp_servers: [review.tracker]\n", "")
        .replace("    skills: [review.checklist]\n", "")
        .replace("    target: review.reviewer:fixture:review.careful\n", "");
    fs::write(machine_path, machine).expect("strip public settings references");
}

fn public_use_case(
    root: &Path,
    label: &str,
    state_reference: &str,
    fragments: &[&str],
    failures: &mut Vec<String>,
) {
    let case = root.join(label);
    fs::create_dir_all(&case).expect("public use case directory");
    let fixture = write_exposure_fixture(&case);
    let wrapper = write_observing_wrapper(&case, "observer-flow", &fixture.leaf, state_reference);
    strip_other_public_references(&wrapper);
    let output = case.join("output");
    let result = instantiate_curated(&case, &wrapper, &output);
    record_failure(failures, label, &result, &output, fragments);
}

/// Uses cannot cross kinds, guess private/generated names, or bypass a wrapper
/// boundary, and diagnostics list the immediate public alternatives.
/// §FS-rhei-library.1.2, §FS-rhei-library.8
#[test]
fn invalid_public_uses_are_denied_at_the_immediate_typed_boundary() {
    let dir = unique_temp_dir("blocks-expose-invalid-uses");
    let mut failures = Vec::new();
    public_use_case(
        &dir,
        "unknown-public-state",
        "review.absent",
        &["review.absent", "state", "review.ready", "public"],
        &mut failures,
    );
    public_use_case(
        &dir,
        "wrong-kind",
        "review.audit",
        &["review.audit", "task", "state", "review.ready"],
        &mut failures,
    );
    public_use_case(
        &dir,
        "private-state",
        "review.internal-ready",
        &["review.internal-ready", "private", "review.ready", "expose"],
        &mut failures,
    );

    let nested = dir.join("nested-bypass");
    fs::create_dir_all(&nested).expect("nested case");
    let fixture = write_exposure_fixture(&nested);
    let inner = nested.join("inner-wrapper");
    fs::create_dir_all(&inner).expect("inner wrapper");
    write_fixture_file(
        &inner,
        "template.yaml",
        &format!(
            "name: inner-wrapper\nversion: 1\ndescription: Private boundary\nports:\n  entry: review.entry\n  exits: {{ done: review.done }}\nuse:\n  - {{ block: {}, as: review }}\n",
            fixture.leaf.display()
        ),
    );
    let outer = write_observing_wrapper(&nested, "outer-wrapper", &inner, "review.review.ready");
    strip_other_public_references(&outer);
    let output = nested.join("output");
    let result = instantiate_curated(&nested, &outer, &output);
    record_failure(
        &mut failures,
        "nested-bypass",
        &result,
        &output,
        &["review.review.ready", "immediate", "re-expose", "inner-wrapper"],
    );
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// Selected declarations receive the same post-render target validation as
/// static declarations; failure is about the target, not generic rejection of
/// the `expose` group. §FS-rhei-library.1.1–1.2, §FS-rhei-library.8
#[test]
fn selected_exposure_reports_its_unresolved_target_after_rendering() {
    let dir = unique_temp_dir("blocks-expose-selected-invalid");
    let fixture = write_exposure_fixture(&dir);
    let manifest_path = fixture.leaf.join("template.yaml");
    let manifest = fs::read_to_string(&manifest_path).expect("leaf manifest");
    let expose = manifest.find("expose:\n").expect("expose declaration");
    fs::write(
        &manifest_path,
        format!(
            "{}select: |\n  expose:\n    states:\n      ready: {{ local: selected-absent }}\n",
            &manifest[..expose]
        ),
    )
    .expect("selected invalid exposure");
    let output = dir.join("output");
    let result = instantiate_direct(&dir, &fixture.leaf, &output);
    let combined = format!("{}\n{}", result.stdout, result.stderr);
    assert!(!result.status.success(), "selected target should fail:\n{combined}");
    for expected in ["selected-absent", "state", "select", "template.yaml"] {
        assert!(combined.contains(expected), "missing {expected:?} in:\n{combined}");
    }
    assert!(!output.exists(), "selected declaration failure published partial output");
}

/// The whole exposure group may be static or selected, never both.
/// §FS-rhei-library.1.1–1.2, §FS-rhei-library.8
#[test]
fn static_and_selected_exposure_groups_are_mutually_exclusive() {
    let dir = unique_temp_dir("blocks-expose-selected-duplicate");
    let fixture = write_exposure_fixture(&dir);
    let manifest_path = fixture.leaf.join("template.yaml");
    let mut manifest = fs::read_to_string(&manifest_path).expect("leaf manifest");
    manifest
        .push_str("select: |\n  expose:\n    states:\n      selected: { local: internal-ready }\n");
    fs::write(&manifest_path, manifest).expect("duplicate selected exposure");
    let output = dir.join("output");
    let result = instantiate_direct(&dir, &fixture.leaf, &output);
    let combined = format!("{}\n{}", result.stdout, result.stderr);
    assert!(!result.status.success(), "duplicate exposure groups should fail:\n{combined}");
    for expected in ["expose", "both static and selected", "template.yaml"] {
        assert!(combined.contains(expected), "missing {expected:?} in:\n{combined}");
    }
    assert!(!output.exists(), "duplicate exposure groups published partial output");
}

/// A child re-exposure must resolve an immediate child's public table.
/// §FS-rhei-library.1.2, §FS-rhei-library.8
#[test]
fn unresolved_child_exposure_names_the_mount_chain_and_public_alternatives() {
    let dir = unique_temp_dir("blocks-expose-unresolved-child");
    let fixture = write_exposure_fixture(&dir);
    let wrapper = dir.join("child-wrapper");
    fs::create_dir_all(&wrapper).expect("child wrapper");
    write_fixture_file(
        &wrapper,
        "template.yaml",
        &format!(
            "name: child-wrapper\nversion: 1\ndescription: Bad re-exposure\nports:\n  entry: review.entry\n  exits: {{ done: review.done }}\nuse:\n  - {{ block: {}, as: review }}\nexpose:\n  states:\n    approved: {{ mount: review, name: absent }}\n",
            fixture.leaf.display()
        ),
    );
    let output = dir.join("output");
    let result = instantiate_curated(&dir, &wrapper, &output);
    let combined = format!("{}\n{}", result.stdout, result.stderr);
    assert!(!result.status.success(), "unresolved child target should fail:\n{combined}");
    for expected in ["approved", "review.absent", "review.ready", "child-wrapper", "state"] {
        assert!(combined.contains(expected), "missing {expected:?} in:\n{combined}");
    }
    assert!(!output.exists(), "unresolved re-exposure published partial output");
}

/// Static and selected schema failures keep the same typed path, authored
/// manifest and nested mount context. §FS-rhei-library.8
#[test]
fn closed_exposure_schema_errors_keep_paths_in_static_and_selected_mounts() {
    let dir = unique_temp_dir("blocks-exposure-schema-context");
    let mut failures = Vec::new();
    for selected in [false, true] {
        for (label, from, to, path, offending, alternative) in [
            (
                "state-field",
                "ready: { local: internal-ready }",
                "ready: { local: internal-ready, surprise: true }",
                "expose.states.ready",
                "surprise",
                "local",
            ),
            (
                "settings-registry",
                "    skills:",
                "    secrets:",
                "expose.settings",
                "secrets",
                "skills",
            ),
            (
                "settings-target",
                "reviewer: { local: internal-agent }",
                "reviewer: { local: internal-agent, surprise: true }",
                "expose.settings.agents.reviewer",
                "surprise",
                "mount",
            ),
            (
                "target-type",
                "ready: { local: internal-ready }",
                "ready: { local: [internal-ready] }",
                "expose.states.ready",
                "sequence",
                "string",
            ),
        ] {
            let label = format!("{label}-selected-{selected}");
            let case = dir.join(&label);
            let fixture = write_exposure_fixture(&case);
            let manifest_path = fixture.leaf.join("template.yaml");
            replace_manifest(&manifest_path, from, to);
            if selected {
                let manifest = fs::read_to_string(&manifest_path).unwrap();
                let (header, exposure) = manifest.split_once("expose:\n").unwrap();
                let mut body = String::from("  expose:\n");
                for line in exposure.lines() {
                    body.push_str("  ");
                    body.push_str(line);
                    body.push('\n');
                }
                fs::write(&manifest_path, format!("{header}select: |\n{body}")).unwrap();
            }
            let wrapper =
                write_observing_wrapper(&case, "observer-flow", &fixture.leaf, "review.ready");
            let output = case.join("output");
            let result = instantiate_curated(&case, &wrapper, &output);
            let mut fragments = vec![
                path,
                offending,
                alternative,
                "template.yaml",
                "mount",
                "observer-flow.review",
            ];
            if selected {
                fragments.push("select");
            }
            record_failure(&mut failures, &label, &result, &output, &fragments);
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
