use std::fs;
use std::path::Path;

use super::block_composition_support::*;
use super::*;

fn instantiate_passes(
    dir: &Path,
    output: &Path,
    review: &Path,
    fix: &Path,
    passes: &[&str],
) -> CliRun {
    let review = mount_arg("review", review);
    let fix = mount_arg("fix", fix);
    let mut args = vec![
        "instantiate",
        "--mount",
        review.as_str(),
        "--mount",
        fix.as_str(),
        "--seam",
        "review.done=fix.entry",
    ];
    for pass in passes {
        args.extend(["--pass", pass]);
    }
    args.extend(["--output", output.to_str().expect("output path")]);
    run_compose(dir, &args)
}

fn transition(output: &Path, task: &str, from: &str, to: &str) -> CliRun {
    run_cli_without_machine(
        "transition",
        output,
        &["--task", task, "--from", from, "--to", to, "--no-callbacks"],
    )
}

/// A file pass uses one qualified runtime path and delegates all enforcement
/// to the ordinary state-artifact runtime: producer output, required consumer
/// input, and optional consumer input each retain their existing rules.
/// §FS-rhei-library.6 docs/functional-spec/rhei-library.spec.md:237
#[test]
fn state_file_pass_is_a_real_runtime_handoff_with_existing_requiredness() {
    let dir = unique_temp_dir("blocks-runtime-file-pass");
    let fixtures = write_composition_fixtures(&dir);
    let required = dir.join("required");
    assert_success(&instantiate_passes(
        &dir,
        &required,
        &fixtures.review,
        &fixtures.fix,
        &["review.report=fix.report"],
    ));

    let task = "m6_review__job";
    let review_state = "m6_review__review";
    let review_done = "m6_review__done";
    let fix_state = "m3_fix__review";
    let artifact = required.join("runtime/blocks/m6_review__/runtime/report.md");

    let missing_output = transition(&required, task, review_state, review_done);
    assert_failed_with(&missing_output, &["report", artifact.to_str().expect("artifact path")]);

    fs::create_dir_all(artifact.parent().expect("artifact parent")).expect("artifact directory");
    fs::write(&artifact, "real review findings\n").expect("write producer output");
    assert_success(&transition(&required, task, review_state, review_done));
    fs::remove_file(&artifact).expect("remove handoff before consumer entry");
    let missing_input = transition(&required, task, review_done, fix_state);
    assert_failed_with(&missing_input, &["report", artifact.to_str().expect("artifact path")]);

    let optional_template =
        fs::read_to_string(fixtures.fix.join("states.yaml")).expect("fix states");
    fs::write(
        fixtures.fix.join("states.yaml"),
        optional_template.replace(
            "{ name: report, path: runtime/report.md }",
            "{ name: report, path: runtime/report.md, optional: true }",
        ),
    )
    .expect("make consumer input optional");
    let optional = dir.join("optional");
    assert_success(&instantiate_passes(
        &dir,
        &optional,
        &fixtures.review,
        &fixtures.fix,
        &["review.report=fix.report"],
    ));
    let optional_artifact = optional.join("runtime/blocks/m6_review__/runtime/report.md");
    fs::create_dir_all(optional_artifact.parent().expect("artifact parent"))
        .expect("artifact directory");
    fs::write(&optional_artifact, "temporary\n").expect("producer output");
    assert_success(&transition(&optional, task, review_state, review_done));
    fs::remove_file(optional_artifact).expect("remove optional handoff");
    assert_success(&transition(&optional, task, review_done, fix_state));
}

fn remove_file_port(fix: &Path) {
    let manifest = fs::read_to_string(fix.join("template.yaml")).expect("fix manifest");
    fs::write(
        fix.join("template.yaml"),
        manifest.replace("    report: { kind: state-file, state: review, name: report }\n", ""),
    )
    .expect("remove file endpoint");
    let states = fs::read_to_string(fix.join("states.yaml")).expect("fix states");
    fs::write(
        fix.join("states.yaml"),
        states.replace("    inputs:\n      - { name: report, path: runtime/report.md }\n", ""),
    )
    .expect("remove file input");
}

fn write_runtime_agent(output: &Path) {
    let agent = write_python_agent(
        output,
        "capture-export.py",
        r###"prompt = sys.stdin.read()
write(pathlib.Path(env("RHEI_ROOT")) / "runtime" / "export-prompt.txt", prompt)
result("## Result\n\nConsumer completed.\n")
"###,
    );
    let settings_dir = output.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("settings directory");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{"agent": "capture", "agent_timeout": "10s"}},
  "agents": {{"capture": {{"command": {}, "stdin_prompt": true, "timeout": "10s"}}}}
}}
"#,
            fixture_command(&agent)
        ),
    )
    .expect("runtime settings");
}

fn run_export_case(dir: &Path, label: &str, contents: Option<&str>) -> String {
    let fixture_root = dir.join(format!("fixtures-{label}"));
    fs::create_dir_all(&fixture_root).expect("fixture root");
    let fixtures = write_composition_fixtures(&fixture_root);
    remove_file_port(&fixtures.fix);
    let output = dir.join(format!("output-{label}"));
    assert_success(&instantiate_passes(
        dir,
        &output,
        &fixtures.review,
        &fixtures.fix,
        &["review.findings=fix.findings"],
    ));

    let plan = super::block_composition_tests::text_files_below_for_runtime(&output);
    assert!(
        plan.contains("**Provides:** m6_review__findings"),
        "missing qualified Provides:\n{plan}"
    );
    assert!(
        plan.contains("**Consumes:** m6_review__job:m6_review__findings"),
        "missing qualified Consumes:\n{plan}"
    );

    if let Some(contents) = contents {
        let export = output.join("runtime/exports/m6_review__job/m6_review__findings.md");
        fs::create_dir_all(export.parent().expect("export parent")).expect("export directory");
        fs::write(export, contents).expect("write export");
    }
    write_runtime_agent(&output);
    let run =
        run_compose(&output, &["run", ".", "--task", "m3_fix__job", "--no-tui", "--no-callbacks"]);
    assert_success(&run);
    fs::read_to_string(output.join("runtime/export-prompt.txt")).expect("captured prompt")
}

/// Export passes lower to qualified Provides/Consumes and then use the existing
/// producer-root lookup and absent/empty skip behavior. §FS-rhei-library.6
#[test]
fn task_export_pass_uses_runtime_content_and_skips_missing_or_empty_exports() {
    let dir = unique_temp_dir("blocks-runtime-export-pass");
    let present = run_export_case(&dir, "present", Some("decisive review findings\n"));
    assert!(present.contains("decisive review findings"), "export absent from prompt:\n{present}");

    let missing = run_export_case(&dir, "missing", None);
    assert!(!missing.contains("decisive review findings"));
    assert!(!missing.contains("m6_review__findings"), "missing export created a prompt section");

    let empty = run_export_case(&dir, "empty", Some(""));
    assert!(!empty.contains("m6_review__findings"), "empty export created a prompt section");
}
