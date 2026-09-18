use std::fs;

use super::block_exposure_support::*;
use super::*;

/// Direct composition consumes the complete author-owned public table and
/// lowers every exposed kind under its public key. §FS-rhei-library.1.2,
/// §FS-rhei-library.3–4
#[test]
fn direct_mount_exposes_state_task_and_each_settings_registry() {
    let dir = unique_temp_dir("blocks-expose-direct");
    let fixture = write_exposure_fixture(&dir);
    let output = dir.join("output");
    let result = instantiate_direct(&dir, &fixture.leaf, &output);
    assert_success(&result);

    let generated = generated_text(&output);
    for public in [
        "m6_review__ready",
        "m6_review__audit",
        "m6_review__reviewer",
        "m6_review__careful",
        "m6_review__tracker",
        "m6_review__checklist",
    ] {
        assert!(generated.contains(public), "missing public identity {public:?} in:\n{generated}");
    }
    for private_target in [
        "m6_review__internal-ready",
        "m6_review__audit-internal",
        "m6_review__internal-agent",
        "m6_review__internal-model",
        "m6_review__internal-tracker",
        "m6_review__internal-skill",
    ] {
        assert!(
            !generated.contains(private_target),
            "exposed target leaked its private generated identity {private_target:?} in:\n{generated}"
        );
    }
    assert!(generated.contains("**State:** m6_review__ready-2"));
    assert!(generated.contains("state: m6_review__ready"));
}

/// A curated wrapper can use public state identities in every typed state
/// position, a public task in `Prior`, and each public settings registry.
/// §FS-rhei-library.1.2, §FS-rhei-library.4
#[test]
fn curated_mount_resolves_typed_public_references_and_counted_states() {
    let dir = unique_temp_dir("blocks-expose-curated");
    let fixture = write_exposure_fixture(&dir);
    let wrapper = write_observing_wrapper(&dir, "observer-flow", &fixture.leaf, "review.ready");
    let output = dir.join("output");
    let result = instantiate_curated(&dir, &wrapper, &output);
    assert_success(&result);

    let generated = generated_text(&output);
    for reference in [
        "**State:** m6_review__ready-2",
        "**Prior:** Task m6_review__audit",
        "from: m6_review__ready",
        "initial: m6_review__ready",
        "state: m6_review__ready",
        "agent: m6_review__reviewer",
        "model: m6_review__careful",
        "target: m6_review__reviewer:fixture:m6_review__careful",
        "m6_review__tracker",
        "m6_review__checklist",
    ] {
        assert!(
            generated.contains(reference),
            "missing rewritten reference {reference:?} in:\n{generated}"
        );
    }
}

/// Repointing an exposure after an internal rename preserves its authored and
/// generated public identities. §FS-rhei-library.1.2, §FS-rhei-library.4
#[test]
fn public_identity_is_stable_across_internal_target_renames() {
    let dir = unique_temp_dir("blocks-expose-stability");
    let first = write_exposure_leaf(&dir, "review-before", "internal-ready");
    let second = write_exposure_leaf(&dir, "review-after", "renamed-ready");
    let before = dir.join("before");
    let after = dir.join("after");
    assert_success(&instantiate_direct(&dir, &first, &before));
    assert_success(&instantiate_direct(&dir, &second, &after));

    let before = generated_text(&before);
    let after = generated_text(&after);
    for generated in [&before, &after] {
        assert!(generated.contains("m6_review__ready"));
    }
    assert!(!before.contains("m6_review__internal-ready"));
    assert!(!after.contains("m6_review__renamed-ready"));
}

/// `select` may produce the whole exposure group after inputs are resolved.
/// §FS-rhei-library.1.1–1.2
#[test]
fn input_selection_can_supply_the_whole_exposure_group() {
    let dir = unique_temp_dir("blocks-expose-selected");
    let fixture = write_exposure_fixture(&dir);
    let manifest_path = fixture.leaf.join("template.yaml");
    let manifest = fs::read_to_string(&manifest_path).expect("leaf manifest");
    let expose = manifest.find("expose:\n").expect("expose declaration");
    let selected = format!(
        "{}inputs:\n  - name: public\n    description: Enable the public surface\n    type: boolean\n    default: true\nselect: |\n  expose:\n  {{% if public %}}\n    states:\n      ready: {{ local: internal-ready }}\n    tasks:\n      audit: {{ local: audit-internal }}\n    settings:\n      agents: {{ reviewer: {{ local: internal-agent }} }}\n      models: {{ careful: {{ local: internal-model }} }}\n      mcp_servers: {{ tracker: {{ local: internal-tracker }} }}\n      skills: {{ checklist: {{ local: internal-skill }} }}\n  {{% endif %}}\n",
        &manifest[..expose]
    );
    fs::write(&manifest_path, selected).expect("select exposure manifest");

    let output = dir.join("output");
    let result = instantiate_direct(&dir, &fixture.leaf, &output);
    assert_success(&result);
    let generated = generated_text(&output);
    assert!(generated.contains("m6_review__ready"));
    assert!(generated.contains("m6_review__checklist"));
}

/// Exposure resolves before the wrapper's final compatibility rename.
/// §FS-rhei-library.7
#[test]
fn compatibility_can_rename_an_exposed_public_identity() {
    let dir = unique_temp_dir("blocks-expose-compatibility");
    let fixture = write_exposure_fixture(&dir);
    let wrapper = dir.join("legacy-wrapper");
    fs::create_dir_all(&wrapper).expect("legacy wrapper");
    write_fixture_file(
        &wrapper,
        "template.yaml",
        &format!(
            r#"name: legacy-wrapper
version: 1
description: Preserve one exposed identity
ports:
  entry: review.entry
  exits: {{ done: review.done }}
use:
  - {{ block: {}, as: review }}
compatibility:
  states: {{ legacy-ready: review.ready }}
"#,
            fixture.leaf.display()
        ),
    );
    let output = dir.join("output");
    let result = instantiate_curated(&dir, &wrapper, &output);
    assert_success(&result);
    let generated = generated_text(&output);
    assert!(generated.contains("legacy-ready:"));
    assert!(!generated.contains("m6_review__ready:"));
}

/// A nested wrapper creates a new boundary and must explicitly re-expose its
/// child's public member. §FS-rhei-library.1.2
#[test]
fn nested_wrapper_can_explicitly_reexpose_an_immediate_child() {
    let dir = unique_temp_dir("blocks-expose-nested");
    let fixture = write_exposure_fixture(&dir);
    let inner = dir.join("inner-wrapper");
    fs::create_dir_all(&inner).expect("inner wrapper");
    write_fixture_file(
        &inner,
        "template.yaml",
        &format!(
            r#"name: inner-wrapper
version: 1
description: Re-expose a child state
ports:
  entry: review.entry
  exits: {{ done: review.done }}
use:
  - {{ block: {}, as: review }}
expose:
  states:
    approved: {{ mount: review, name: ready }}
"#,
            fixture.leaf.display()
        ),
    );
    let outer = write_observing_wrapper(&dir, "outer-wrapper", &inner, "review.approved");
    let output = dir.join("output");
    let result = instantiate_curated(&dir, &outer, &output);
    assert_success(&result);
    let generated = generated_text(&output);
    assert!(generated.contains("m6_review__approved"));
    assert!(!generated.contains("m6_review__m6_review__ready"));
}

/// Exposing a task permits an identity dependency, not access to its exports.
/// A pass through declared data remains required. §FS-rhei-library.1.2
#[test]
fn exposed_task_does_not_implicitly_expose_its_exports() {
    let dir = unique_temp_dir("blocks-expose-task-export");
    let fixture = write_exposure_fixture(&dir);
    let wrapper = write_observing_wrapper(&dir, "export-reader", &fixture.leaf, "review.ready");
    let task_path = wrapper.join("tasks/01-observe.md");
    let task = fs::read_to_string(&task_path).expect("observer task");
    fs::write(
        &task_path,
        task.replace(
            "**Prior:** Task review.audit\n",
            "**Prior:** Task review.audit\n**Consumes:** review.audit:secret\n",
        ),
    )
    .expect("unauthorized export consumer");
    let result = instantiate_curated(&dir, &wrapper, &dir.join("output"));
    let combined = format!("{}\n{}", result.stdout, result.stderr);
    assert!(!result.status.success(), "private export access should fail:\n{combined}");
    for expected in ["review.audit", "secret", "data", "pass"] {
        assert!(combined.contains(expected), "missing {expected:?} in:\n{combined}");
    }
}
