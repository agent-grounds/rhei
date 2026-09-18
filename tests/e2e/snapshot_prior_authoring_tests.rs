//! Task inheritance authoring and JSON rendering.
//! §FS-rhei-new.1.3 §FS-rhei-render.3.1

use std::fs;

use super::new_tests::{new_run, project_with_rhei};
use super::*;

#[test]
fn snapshot_prior_new_authors_task_inheritance_and_render_normalizes_it() {
    let dir = project_with_rhei("new-ticket-inherits");
    assert_success(&new_run(&["new", "Source", "--under", "auth"], &dir));
    let created = new_run(
        &[
            "new",
            "Consumer",
            "--under",
            "auth",
            "--prior",
            "auth.1",
            "--inherits",
            "reviewed from prior",
            "--provides",
            "fix",
        ],
        &dir,
    );
    assert_success(&created);

    let plan = fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file");
    assert!(
        plan.contains("**Prior:** auth.1\n**Inherits:** reviewed from prior\n**Provides:** fix\n"),
        "metadata should be in grammar order:\n{plan}"
    );

    let rendered = new_run(&["render", "auth.rhei.md", "--format", "json"], &dir);
    assert_success(&rendered);
    let json: serde_json::Value =
        serde_json::from_str(&rendered.stdout).expect("rendered JSON should parse");
    let tasks = json["tasks"].as_array().expect("tasks array");
    assert!(tasks[0].get("inherits").is_none(), "omission must remain absent: {json}");
    assert_eq!(tasks[1]["inherits"], "reviewed from prior");

    let opted_out =
        new_run(&["new", "Independent review", "--under", "auth", "--inherits", "none"], &dir);
    assert_success(&opted_out);
    let plan = fs::read_to_string(dir.join("auth.rhei.md")).expect("rhei file");
    assert!(plan.contains("**Inherits:** none\n"), "explicit opt-out should be authored:\n{plan}");
}
