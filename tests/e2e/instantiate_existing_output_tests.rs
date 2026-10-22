//! A dry run refuses an existing output the way the real write does.
//!
//! Port of the #507 reproducer: `--dry-run` over an existing destination
//! exited 0 and printed `Instantiate this template with:` followed by the very
//! command the real write then refused. Both wordings of the refusal are
//! covered, an explicit `--output` and the default `<cwd>/<template-name>`, and
//! in both the destination's bytes must survive. §FS-rhei-templates.6.1.2

use std::fs;
use std::path::Path;

use super::instantiate_project_validation_support::snapshot;
use super::templates_tests::run_raw;
use super::*;

const SENTINEL: &str = "the existing destination belongs to its owner\n";

/// The built-in `fix` template, by path: it needs no values.
fn fix_template() -> String {
    repo_root().join("crates/rhei-cli/templates/fix").to_str().unwrap().to_owned()
}

/// Dry-run first, then the real write with the same arguments, from `cwd`,
/// asserting the destination is untouched after each.
fn dry_then_real(cwd: &Path, destination: &Path, args: &[&str]) -> (CliRun, CliRun) {
    let before = snapshot(destination);
    let dry_args: Vec<&str> = args.iter().copied().chain(["--dry-run"]).collect();
    let dry = run_raw(&dry_args, cwd);
    assert_eq!(snapshot(destination), before, "dry run altered the existing destination");
    let real = run_raw(args, cwd);
    assert_eq!(snapshot(destination), before, "real refusal altered the existing destination");
    (dry, real)
}

fn assert_dry_run_refuses_like_real(dry: &CliRun, real: &CliRun, wording: &str) {
    assert_eq!(real.status.code(), Some(1), "the real write refuses:\n{}", real.stderr);
    assert!(real.stderr.contains(wording), "real refusal wording:\n{}", real.stderr);
    assert_eq!(
        dry.status.code(),
        Some(1),
        "dry run must refuse the existing output the real write refuses\ndry stdout:\n{}\n\
         dry stderr:\n{}\nreal stderr:\n{}",
        dry.stdout,
        dry.stderr,
        real.stderr
    );
    assert_eq!(dry.stderr, real.stderr, "dry run reports the real write's error");
    assert!(
        !dry.stdout.contains("Instantiate this template with:"),
        "a refused dry run offers no runnable invocation:\n{}",
        dry.stdout
    );
}

/// An explicit existing `--output`: "output path '…' already exists".
/// §FS-rhei-templates.6.1.2
#[test]
fn dry_run_refuses_existing_explicit_output_like_real_write() {
    let dir = unique_temp_dir("instantiate-existing-explicit");
    let output = dir.join("out");
    fs::create_dir(&output).unwrap();
    write_fixture_file(&output, "sentinel.txt", SENTINEL);
    let template = fix_template();
    let args = ["instantiate", &template, "--output", output.to_str().unwrap()];
    let (dry, real) = dry_then_real(&dir, &output, &args);
    let wording = format!("output path '{}' already exists", output.display());
    assert_dry_run_refuses_like_real(&dry, &real, &wording);
}

/// The default output, `<cwd>/fix`, already exists: "has already been
/// instantiated here under that name". §FS-rhei-templates.6.1.2
#[test]
fn dry_run_refuses_existing_default_output_like_real_write() {
    let dir = unique_temp_dir("instantiate-existing-default");
    let output = dir.join("fix");
    fs::create_dir(&output).unwrap();
    write_fixture_file(&output, "sentinel.txt", SENTINEL);
    let template = fix_template();
    let (dry, real) = dry_then_real(&dir, &output, &["instantiate", &template]);
    assert_dry_run_refuses_like_real(
        &dry,
        &real,
        "has already been instantiated here under that name",
    );
}

/// `--into <project>` over an existing member `<project>/fix` refuses alike
/// dry and real, as the default output does. §FS-rhei-library.2.2
#[test]
fn dry_run_refuses_existing_into_project_member_like_real_write() {
    let dir = unique_temp_dir("instantiate-existing-into-member");
    let project = dir.join("proj");
    fs::create_dir(&project).unwrap();
    write_fixture_file(&project, "index.panta.md", "# Panta: Existing member\n");
    let member = project.join("fix");
    fs::create_dir(&member).unwrap();
    write_fixture_file(&member, "sentinel.txt", SENTINEL);
    let template = fix_template();
    let args = ["instantiate", &template, "--into", project.to_str().unwrap()];
    let (dry, real) = dry_then_real(&dir, &member, &args);
    assert_dry_run_refuses_like_real(
        &dry,
        &real,
        "has already been instantiated here under that name",
    );
}
