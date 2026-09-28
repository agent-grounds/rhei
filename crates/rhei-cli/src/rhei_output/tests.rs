use super::*;
use rhei_core::parser::parse;

#[test]
fn json_output_minimal_smoke() {
    let input = r#"# Rhei: Minimal

## Tasks

### Task 1: Alpha
**State:** pending
"#;

    let rhei = parse(input).expect("parse ok");
    let v = to_json_value(&rhei).expect("every frontmatter value has a JSON image");

    assert_eq!(v["title"].as_str().unwrap(), "Minimal");
    assert!(v["content_sections"].is_array());

    let tasks = v["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["id"]["path"].as_str(), Some("1"));
    assert_eq!(tasks[0]["id"]["segments"][0]["number"].as_u64(), Some(1));
    assert_eq!(tasks[0]["kind"].as_str(), Some("task"));
    assert_eq!(tasks[0]["state"].as_str(), Some("pending"));
    assert!(tasks[0]["children"].as_array().unwrap().is_empty());
}

#[test]
fn json_output_with_named_ids_and_children_and_prior() {
    let input = r#"# Rhei: Complex

## Tasks

### Task 1: First
**State:** pending

#### Task 1.1: Do A
**State:** pending
Do A line

#### Task 1.2: Do B
**State:** completed
Do B line

### Task build: Build it
**State:** pending
**Prior:** Task 1
"#;

    let rhei = parse(input).expect("parse ok");
    let v = to_json_value(&rhei).expect("every frontmatter value has a JSON image");

    let tasks = v["tasks"].as_array().unwrap();

    let build = tasks
        .iter()
        .find(|t| t["id"]["path"].as_str() == Some("build"))
        .expect("build task exists");

    let deps = build["prior"].as_array().unwrap();
    assert!(!deps.is_empty());
    assert_eq!(deps[0]["path"].as_str(), Some("1"));

    let task1 =
        tasks.iter().find(|t| t["id"]["path"].as_str() == Some("1")).expect("task 1 exists");

    let children = task1["children"].as_array().unwrap();
    assert_eq!(children.len(), 2);
    assert_eq!(children[0]["id"]["path"].as_str(), Some("1.1"));
    assert_eq!(children[0]["title"].as_str().unwrap(), "Do A");
    assert_eq!(children[1]["id"]["path"].as_str(), Some("1.2"));
    assert_eq!(children[1]["title"].as_str().unwrap(), "Do B");
}

#[test]
fn json_output_omits_assignee_when_absent() {
    let input = r#"# Rhei: NoAssignee

## Tasks

### Task 1: Alpha
**State:** pending
"#;

    let rhei = parse(input).expect("parse ok");
    let v = to_json_value(&rhei).expect("every frontmatter value has a JSON image");
    let tasks = v["tasks"].as_array().unwrap();
    assert!(tasks[0].as_object().unwrap().get("assignee").is_none());
}

#[test]
fn json_output_includes_assignee_when_present() {
    let input = r#"# Rhei: Assigned

## Tasks

### Task 1: Alpha
**State:** in-progress
**Assignee:** alice
"#;

    let rhei = parse(input).expect("parse ok");
    let v = to_json_value(&rhei).expect("every frontmatter value has a JSON image");
    let tasks = v["tasks"].as_array().unwrap();
    assert_eq!(tasks[0]["assignee"].as_str(), Some("alice"));
}

#[test]
fn github_markdown_renders_assignee_line() {
    let input = r#"# Rhei: Assigned

## Tasks

### Task 1: Alpha
**State:** in-progress
**Assignee:** alice
"#;
    let rhei = parse(input).expect("parse ok");
    let s = to_github_markdown(&rhei);
    assert!(s.contains("- Assignee: alice"));
}

#[test]
fn json_output_escaped_state_space() {
    let input = r#"# Rhei: Escape

## Tasks

### Task 1: One
**State:** `in progress`
"#;

    let rhei = parse(input).expect("parse ok");
    let v = to_json_value(&rhei).expect("every frontmatter value has a JSON image");
    let tasks = v["tasks"].as_array().unwrap();
    assert_eq!(tasks[0]["state"].as_str(), Some("in progress"));
}

#[test]
fn missing_state_is_parse_error() {
    let input = r#"# Rhei: NoState

## Tasks

### Task 1: One
"#;

    let err = parse(input).unwrap_err();
    assert!(err.message.contains("missing mandatory **State:**"));
}

#[test]
fn github_markdown_tree_smoke() {
    let input = r#"# Rhei: Minimal

## Tasks

### Task 1: Alpha
**State:** pending

#### Task 1.1: Do it
**State:** pending
"#;
    let rhei = parse(input).expect("parse ok");
    let s = to_github_markdown(&rhei);

    assert!(s.contains("# Rhei: Minimal"));
    assert!(s.contains("## Tasks"));
    assert!(s.contains("### Task 1: Alpha"));
    assert!(s.contains("#### Task 1.1: Do it"));
    assert!(s.contains("- State: pending"));
}

#[test]
fn progress_report_basic_colors_and_structure() {
    let input = r#"# Rhei: Progress
## Tasks

### Task 1: Alpha
**State:** pending

#### Task 1.1: Do it
**State:** pending
"#;

    let rhei = parse(input).expect("parse ok");
    let gen = ProgressReportOutput::plain(true, true);
    let s = gen.to_string(&rhei);

    assert!(s.contains("Rhei: "));
    assert!(s.contains("* Task 1: Alpha"));
    assert!(s.contains("[PENDING]"));
    assert!(s.contains("\x1b["));
}

#[test]
fn progress_report_includes_dependencies() {
    let input = r#"# Rhei: Prior
## Tasks

### Task 1: One
**State:** completed

### Task 2: Two
**State:** in progress
**Prior:** Task 1
"#;

    let rhei = parse(input).expect("parse ok");
    let s = to_progress_report(&rhei);

    assert!(s.contains("* Task 2: Two"));
    let prior_line = "  - Prior: Task 1";
    assert!(s.contains(prior_line));
}

#[test]
fn progress_report_no_color_option() {
    let input = r#"# Rhei: NoColor
## Tasks

### Task 1: Alpha
**State:** pending
"#;

    let rhei = parse(input).expect("parse ok");
    let gen = ProgressReportOutput::plain(false, true);
    let s = gen.to_string(&rhei);

    assert!(!s.contains("\x1b["));
    assert!(s.contains("[PENDING]"));
}
