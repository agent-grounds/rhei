//! Diagnostics retain endpoints, sources, and nested mount chains. §FS-rhei-library.8
use super::*;

fn flow() -> Block {
    let mut review = simple("review-block");
    review.local.as_mut().unwrap().machine.states.get_mut("work").unwrap().outputs =
        serde_yaml::from_str("[{name: report, path: runtime/report.md}]").unwrap();
    review.manifest.data.outputs.insert(
        "report".into(),
        DataEndpoint {
            kind: DataKind::StateFile,
            state: Some("work".into()),
            task: None,
            name: "report".into(),
        },
    );
    let mut fix = simple("fix-block");
    fix.manifest.data.inputs.insert(
        "findings".into(),
        DataEndpoint {
            kind: DataKind::TaskExport,
            state: None,
            task: Some("job".into()),
            name: "findings".into(),
        },
    );
    let mut flow = group(vec![("review", review), ("fix", fix)]);
    expose(&mut flow, "review", "fix");
    flow.manifest.seams = Some(vec![Seam {
        from: "review.done".into(),
        to: "fix.entry".into(),
        pass: BTreeMap::new(),
    }]);
    flow
}

fn nested_failure(flow: Block, expected: &[&str]) {
    let mut outer = group(vec![("outer", flow)]);
    outer.source = "/outer/template.yaml".into();
    let error = outer.compile().unwrap_err();
    for part in expected.iter().copied().chain([
        "/review-block/template.yaml",
        "/fix-block/template.yaml",
        "outer.review",
        "outer.fix",
    ]) {
        assert!(error.contains(part), "missing {part}: {error}");
    }
}

#[test]
fn unknown_control_and_data_ports_retain_both_manifests_and_alternatives() {
    let mut bad_control = flow();
    bad_control.manifest.seams.as_mut().unwrap()[0].from = "review.internal".into();
    nested_failure(bad_control, &["review.internal", "review.done", "fix.entry"]);
    let mut bad_data = flow();
    bad_data.manifest.seams.as_mut().unwrap()[0]
        .pass
        .insert("review.unknown".into(), "fix.findings".into());
    nested_failure(bad_data, &["review.unknown", "review.report", "fix.findings"]);
}

#[test]
fn kind_mismatch_retains_both_named_endpoints_after_merging_children() {
    let mut flow = flow();
    flow.manifest.seams.as_mut().unwrap()[0]
        .pass
        .insert("review.report".into(), "fix.findings".into());
    nested_failure(
        flow,
        &["review.report", "fix.findings", "state-file", "task-export", "use matching endpoints"],
    );
}
