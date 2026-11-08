//! §FS-rhei-errors.1.2.3: values-file help follows a scan of every document,
//! retaining the file and loading cause without inventing syntax or a position.

use super::*;

/// Port the issue's bundled-template invocation: these inputs must reach the
/// values loader and refuse before any output can be published.
fn refusal(text: &str, cause: &str) -> (String, String) {
    let dir = unique_temp_dir("values-file-help");
    let path = write_fixture_file(&dir, "values.yaml", text);
    let run = templates_tests::run_raw(
        &[
            "instantiate",
            "changeset-review",
            "HEAD~1",
            "--values",
            path.to_str().expect("values path"),
            "--output",
            "out",
        ],
        &dir,
    );
    assert_eq!(run.status.code(), Some(1), "stdout: {}\nstderr: {}", run.stdout, run.stderr);
    assert!(!dir.join("out").exists(), "refused values must not publish output");
    let (headline, help) = run.stderr.split_once("help:").expect("values refusal has help");
    assert!(
        headline.contains("failed to parse values file")
            && headline.contains(path.to_str().expect("values path"))
            && headline.contains(cause),
        "must refuse this values file for {cause:?}, got:\n{}",
        run.stderr
    );
    (headline.to_string(), help.trim().to_lowercase())
}

fn assert_read_help(help: &str, subject: &[&str]) {
    assert!(
        !help.contains("syntax") && !help.contains("position"),
        "a scan-accepted values file needs loading help, got: {help}"
    );
    assert!(!help.contains("valid yaml"), "a read failure makes no YAML validity claim: {help}");
    assert!(
        ["correct", "fix", "resolve", "remove", "replace"]
            .iter()
            .any(|action| help.contains(action))
            && subject.iter().any(|subject| help.contains(subject)),
        "help must direct correction of the loading problem named in the cause: {help}"
    );
}

#[test]
fn overflow_values_file_gets_loading_correction() {
    let (_, help) = refusal("a: 99999999999999999999999\n", "99999999999999999999999");
    assert_read_help(&help, &["above", "reported", "named", "number", "integer"]);
}

#[test]
fn duplicate_values_key_gets_loading_correction() {
    let (_, help) = refusal("a: 1\na: 2\n", "duplicate entry with key \"a\"");
    assert_read_help(&help, &["above", "reported", "named", "duplicate", "key"]);
}

#[test]
fn multiple_values_documents_are_told_to_keep_one() {
    for text in ["a: 1\n---\nb: 2\n", "a: 1\n...\n---\n"] {
        let (_, help) = refusal(text, "more than one document");
        assert!(
            !help.contains("syntax") && !help.contains("position"),
            "scan-accepted documents need no syntax or position help, got: {help}"
        );
        assert!(
            ["one document", "single document", "one yaml document", "single yaml document"]
                .iter()
                .any(|correction| help.contains(correction)),
            "help must explicitly ask for one document: {help}"
        );
        assert!(
            [
                "keep", "use", "write", "combine", "merge", "put", "must", "require", "contain",
                "hold"
            ]
            .iter()
            .any(|action| help.contains(action)),
            "help must give the document-count correction, not just repeat the cause: {help}"
        );
    }
}

fn assert_syntax_help(headline: &str, help: &str) {
    assert!(help.contains("syntax"), "scan-rejected text needs syntax guidance: {help}");
    assert!(
        !help.contains("one document") && !help.contains("single document"),
        "malformed documents need syntax repair before document-count advice: {help}"
    );
    if help.contains("position") {
        // Both coordinates must actually be printed, wherever the scan failed.
        assert!(
            headline.contains("line ") && headline.contains("column "),
            "help must not refer to an unprinted position; headline: {headline}; help: {help}"
        );
    }
}

/// This syntax control already passes before #631; keep it active.
#[test]
fn malformed_values_document_keeps_syntax_help() {
    let (headline, help) = refusal("a: [\n", "while parsing a flow node");
    assert_syntax_help(&headline, &help);
}

#[test]
fn malformed_later_values_documents_keep_syntax_help() {
    // The original cause reports multiple documents; the scanner must inspect
    // the second and third documents rather than stop at the first separator.
    for text in ["a: 1\n---\nb: [\n", "a: 1\n---\nb: 2\n---\nc: [\n"] {
        let (headline, help) = refusal(text, "more than one document");
        assert_syntax_help(&headline, &help);
    }
}

/// §FS-rhei-templates.6.1.2: accepted YAML/JSON, empty files and the input
/// precedence groups still render the selected value and publish output.
#[test]
fn accepted_values_and_input_precedence_are_preserved() {
    let dir = unique_temp_dir("values-file-help-controls");
    let template = dir.join("values-control");
    fs::create_dir(&template).expect("template directory");
    write_fixture_file(
        &template,
        "template.yaml",
        "name: values-control\nversion: 1.0.0\ndescription: Values control\ninputs:\n  - name: target\n    description: Target\n    positional: 1\n    required: false\n    default: default\n",
    );
    write_fixture_file(
        &template,
        "plan.rhei.md",
        "# Rhei: {{target}}\n\n## Tasks\n\n### Task a: Work\n**State:** pending\n",
    );
    for (name, text) in [
        ("empty.yaml", " \n\t\n"),
        ("first.yaml", "target: first\n"),
        ("last.json", "{\"target\": \"last\"}\n"),
        ("content.txt", "from-file"),
    ] {
        write_fixture_file(&dir, name, text);
    }
    let cases: &[(&[&str], &str)] = &[
        (&["--values", "empty.yaml"], "default"),
        (&["--values", "first.yaml"], "first"),
        (&["--values", "first.yaml", "--values", "last.json"], "last"),
        (&["--values", "last.json", "positional"], "positional"),
        (&["--values", "last.json", "target=bare", "--set", "target=flag"], "flag"),
        (&["--set", "target=flag", "--set-file", "target=content.txt"], "from-file"),
    ];
    for (index, (inputs, expected)) in cases.iter().enumerate() {
        let out = format!("out-{index}");
        let mut args = vec!["instantiate", template.to_str().expect("template path")];
        args.extend_from_slice(inputs);
        args.extend(["--output", &out]);
        let run = templates_tests::run_raw(&args, &dir);
        assert_success(&run);
        let plan = fs::read_to_string(dir.join(out).join("plan.rhei.md")).expect("published plan");
        assert!(plan.starts_with(&format!("# Rhei: {expected}\n")), "{inputs:?}: {plan}");
    }
}

/// §FS-rhei-errors.1.2.3: shape checks still apply after a successful value read.
#[test]
fn values_mapping_and_string_key_checks_are_preserved() {
    let dir = unique_temp_dir("values-file-help-shape");
    for (text, cause) in [
        ("[a, b]\n", "must contain a YAML or JSON object at the top level"),
        ("1: value\n", "contains a non-string key"),
    ] {
        let path = write_fixture_file(&dir, "values.yaml", text);
        let run = templates_tests::run_raw(
            &[
                "instantiate",
                "changeset-review",
                "HEAD~1",
                "--values",
                path.to_str().expect("values path"),
                "--output",
                "out",
            ],
            &dir,
        );
        assert_eq!(run.status.code(), Some(1), "{}", run.stderr);
        assert!(run.stderr.contains(cause), "{}", run.stderr);
        assert!(run.stderr.contains(path.to_str().expect("values path")), "{}", run.stderr);
        assert!(!dir.join("out").exists());
    }
}
