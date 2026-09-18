use std::fs;
use std::path::{Path, PathBuf};

use super::block_exposure_support::*;
use super::*;

fn private_wrapper(root: &Path, fields: &str) -> PathBuf {
    let fixture = write_exposure_fixture(root);
    let path = fixture.leaf.join("template.yaml");
    let manifest = fs::read_to_string(&path).unwrap();
    fs::write(path, manifest.split_once("expose:\n").unwrap().0).unwrap();
    let wrapper = root.join("wrapper");
    fs::create_dir_all(&wrapper).unwrap();
    write_fixture_file(&wrapper, "template.yaml", &format!(
        "name: wrapper\nversion: 1\ndescription: Private settings boundary\nports:\n  entry: review.entry\n  exits: {{ done: review.done }}\nuse:\n  - {{ block: {}, as: review }}\n", fixture.leaf.display()
    ));
    write_fixture_file(&wrapper, "index.rhei.md", "# Rhei: Wrapper\n**States:** wrapper\n");
    write_fixture_file(&wrapper, "states.yaml", &format!(
        "name: wrapper\nversion: 1\nstates:\n  observe:\n    description: Observer\n{fields}  finished:\n    final: true\ntransitions:\n  - {{ from: observe, to: finished }}\nprofiles:\n  primary: {{ initial: observe, allowed: [observe, finished] }}\nnode_policy: {{ root: primary, default: primary }}\n"
    ));
    wrapper
}

/// No exposure declaration authorizes using a concrete child setting identity,
/// including either component of an execution target. §FS-rhei-library.1.2, §FS-rhei-library.8
#[test]
fn generated_settings_bypass_is_denied_before_output_publication() {
    let dir = unique_temp_dir("blocks-exposure-generated-settings");
    let baseline = dir.join("baseline");
    let wrapper = private_wrapper(&baseline, "");
    assert_success(&instantiate_curated(&baseline, &wrapper, &baseline.join("output")));
    for (kind, field, private) in [
        ("agents", "agent: REF", "internal-agent"),
        ("models", "model: REF", "internal-model"),
        ("mcp_servers", "mcp_servers: [REF]", "internal-tracker"),
        ("skills", "skills: [REF]", "internal-skill"),
        ("agents", "target: REF:fixture:external-model", "internal-agent"),
        ("models", "target: external-agent:fixture:REF", "internal-model"),
    ] {
        let case = dir.join(format!("{kind}-{}", field.split(':').next().unwrap()));
        let generated = format!("m6_review__{private}");
        let wrapper =
            private_wrapper(&case, &format!("    {}\n", field.replace("REF", &generated)));
        let output = case.join("output");
        let result = instantiate_curated(&case, &wrapper, &output);
        let diagnostic = format!("{}\n{}", result.stdout, result.stderr);
        assert!(!result.status.success(), "{field} bypass succeeded:\n{diagnostic}");
        for fragment in [
            kind,
            generated.as_str(),
            "private",
            "expose",
            "review",
            "template.yaml",
            "none declared",
        ] {
            assert!(diagnostic.contains(fragment), "{field}: missing {fragment:?}:\n{diagnostic}");
        }
        assert!(!output.exists(), "{field} published partial output");
    }
}

/// External references may resemble generated names: only ownership in the
/// relevant immediate child registry closes the boundary. §FS-rhei-library.1.2
#[test]
fn external_settings_with_generated_spelling_survive_nested_composition() {
    let dir = unique_temp_dir("blocks-exposure-external-settings");
    let wrapper = private_wrapper(&dir, "    agent: m6_review__external-agent\n    model: m6_review__external-model\n    mcp_servers: [m6_review__external-tracker]\n    skills: [m6_review__external-skill]\n");
    let outer = dir.join("outer");
    fs::create_dir_all(outer.join("skills/checklist")).unwrap();
    write_fixture_file(&outer, "template.yaml", &format!(
        "name: outer\nversion: 1\ndescription: Supply external settings\nports:\n  entry: inner.entry\n  exits: {{ done: inner.done }}\nuse:\n  - {{ block: {}, as: inner }}\n", wrapper.display()
    ));
    let settings = fs::read_to_string(dir.join("review-block/settings.json"))
        .unwrap()
        .replace("internal-", "m6_review__external-");
    write_fixture_file(&outer, "settings.json", &settings);
    write_fixture_file(&outer, "skills/checklist/SKILL.md", "# External checklist\n");
    let output = dir.join("output");
    assert_success(&instantiate_curated(&dir, &outer, &output));
    let generated = generated_text(&output);
    for reference in [
        "agent: m6_review__external-agent",
        "model: m6_review__external-model",
        "m6_review__external-tracker",
        "m6_review__external-skill",
    ] {
        assert!(generated.contains(reference), "lost external reference {reference}:\n{generated}");
    }
}
