//! What laying a project template into a project that already exists does to
//! the files that project already has: the machine's bundle is replaced with
//! the machine and every file that changes is named, nothing is written through
//! a symbolic link, a refusal names the project's own paths rather than the
//! copies it was checked in, and a failure puts back every byte and mode.
//! §FS-rhei-library.2.2 §FS-rhei-library.2.3

use super::into_project_lay_support::*;
use super::*;

/// A member template whose one ticket holds a state its own machine lacks, so
/// laying it fails validation after the project's root has been written.
fn write_bad_member_template(dir: &Path) -> PathBuf {
    let template = dir.join(".agent-grounds/rhei/templates/bad");
    std::fs::create_dir_all(template.join("tasks")).expect("create the member template");
    write_fixture_file(&template, "template.yaml", "name: bad\nversion: 1.0.0\ndescription: Bad\n");
    write_fixture_file(&template, "states.yaml", &machine("badmachine", "recording", "step"));
    write_fixture_file(
        &template,
        "index.rhei.md",
        "# Rhei: Bad\n\n---\nstructure:\n  maxLevels: 2\n  nodeKinds:\n  - step\n---\n\n## Overview\n\nFails.\n",
    );
    write_fixture_file(
        &template.join("tasks"),
        "001-record.md",
        "### Step record: Holds a state no machine has\n**State:** nosuchstate\n",
    );
    template
}

/// Run `rhei` as [`rhei_in`] does, with its temp directory at `tmp`, so an
/// assertion can say that no path under it reached the output.
fn rhei_with_tmp(dir: &Path, cwd: &Path, tmp: &Path, args: &[&str]) -> CliRun {
    std::fs::create_dir_all(tmp).expect("create the temp directory");
    let output = rhei_command(dir.join(".home"))
        .current_dir(cwd)
        .env("TMPDIR", tmp)
        .env("TMP", tmp)
        .env("TEMP", tmp)
        .args(args)
        .output()
        .expect("rhei command should run");
    CliRun::from(&output)
}

/// The output's `replaced:` line.
fn replaced_line(run: &CliRun) -> String {
    run.stdout
        .lines()
        .find(|line| line.trim_start().starts_with("replaced:"))
        .unwrap_or_else(|| panic!("a `replaced:` line; got:\n{}", run.stdout))
        .to_owned()
}

/// A rebind writes the bundle files the template carries whole, leaves a file
/// it does not carry alone, and names each file whose bytes change — under
/// `--dry-run` before anything is written, and again when it writes.
/// §FS-rhei-library.2.2
#[test]
fn a_rebind_replaces_the_bundle_and_names_each_changed_file() {
    let dir = unique_temp_dir("bundle-replaced");
    let template = write_lifecycle_templates(&dir);
    assert_success(&rhei_in(&dir, &dir, &["instantiate", "lifecycle", "--output", "reports"]));
    let project = dir.join("reports");
    write_fixture_file(&project.join("scripts"), "collect.sh", "#!/bin/sh\necho my own\n");
    write_fixture_file(&project.join("prompt_templates"), "brief.md", "My brief for {subject}.\n");
    write_fixture_file(&project.join("scripts"), "mine.sh", "#!/bin/sh\necho kept\n");
    let before = snapshot(&project);

    let dry = rhei_in(&dir, &dir, &["instantiate", "lifecycle", "--into", "reports", "--dry-run"]);
    assert_success(&dry);
    let line = replaced_line(&dry);
    for named in ["prompt_templates/brief.md", "scripts/collect.sh"] {
        assert!(line.contains(named), "--dry-run names {named}; got:\n{}", dry.stdout);
    }
    assert!(
        !line.contains("mine.sh"),
        "a file the template does not carry is not replaced: {line}"
    );
    assert_same_tree(&before, &snapshot(&project), "--dry-run writes nothing");

    let rebound = rhei_in(&dir, &dir, &["instantiate", "lifecycle", "--into", "reports"]);
    assert_success(&rebound);
    assert_eq!(replaced_line(&rebound), line, "the rebind names what --dry-run named");
    for carried in ["prompt_templates/brief.md", "scripts/collect.sh"] {
        assert_eq!(
            std::fs::read(project.join(carried)).expect("the replaced file"),
            std::fs::read(template.join(carried)).expect("the template's file"),
            "{carried} is written whole"
        );
    }
    assert_eq!(
        std::fs::read_to_string(project.join("scripts/mine.sh")).expect("the uncarried file"),
        "#!/bin/sh\necho kept\n",
        "a file the template does not carry is left as it is"
    );
}

/// `--dry-run` runs the stranding check and refuses as the write would, and
/// writes nothing, not even the lock sidecar. §FS-rhei-library.2.2
#[test]
fn a_dry_run_refuses_a_stranding_rebind_and_takes_no_lock() {
    let dir = unique_temp_dir("bundle-dry-stranding");
    write_lifecycle_templates(&dir);
    let house = machine("housemachine", "work", "task");
    let project = write_project(&dir, "reports", PLAIN_MANIFEST, Some(&house));
    write_fixture_file(
        &project,
        "stranded.rhei.md",
        "# Rhei: Stranded\n\n## Tasks\n\n### Task one: Holds a state the new default lacks\n**State:** work\n",
    );
    let before = snapshot(&project);

    let dry = rhei_in(&dir, &dir, &["instantiate", "lifecycle", "--into", "reports", "--dry-run"]);
    assert!(!dry.status.success(), "a stranding is refused under --dry-run; got:\n{}", dry.stdout);
    assert_stderr_contains(&dry, "stranded.one");
    assert_same_tree(&before, &snapshot(&project), "--dry-run writes nothing");
    assert!(!project.join("index.panta.md.lock").exists(), "--dry-run takes no lock");
}

/// A bundle linked into the template's own directory, as projects were bound by
/// hand, is refused naming each link: nothing is written through it, so the
/// template's source and the project are both byte-identical.
/// §FS-rhei-library.2.2 §REQ-cross-platform.2
#[cfg(unix)]
#[test]
fn a_linked_bundle_is_refused_and_its_target_left_alone() {
    use std::os::unix::fs::symlink;

    let dir = unique_temp_dir("bundle-linked");
    let template = write_project_template(&dir, "linked", &laid_machine(&[]), "");
    write_fixture_file(
        &template,
        "template.yaml",
        "name: linked\nversion: 1.0.0\ndescription: A project lifecycle\ninputs:\n  - name: who\n    description: Who runs it\n    default: alice\n",
    );
    write_fixture_file(&template.join("scripts"), "collect.sh", "#!/bin/sh\necho {{ who }}\n");
    let project = write_project(&dir, "reports", PLAIN_MANIFEST, Some(&laid_machine(&[])));
    for linked in ["prompt_templates", "scripts"] {
        symlink(template.join(linked), project.join(linked)).expect("link the bundle");
    }
    let before = (snapshot(&template), snapshot(&project));

    let rebound = rhei_in(&dir, &dir, &["instantiate", "linked", "--into", "reports"]);
    assert!(!rebound.status.success(), "a linked bundle is refused; got:\n{}", rebound.stdout);
    assert_stderr_contains(&rebound, "reports/scripts");
    assert_stderr_contains(&rebound, "reports/prompt_templates");
    assert_stderr_contains(&rebound, "symbolic link");
    assert_same_tree(&before.0, &snapshot(&template), "the template's source is not written");
    assert_same_tree(&before.1, &snapshot(&project), "a refused rebind writes nothing");
}

/// A project's deprecated settings file is warned about once, by its own path:
/// the check reads the real project before it reads a copy, and the copy says
/// nothing of its own. §FS-rhei-templates.1.3 §FS-rhei-library.2.3
#[test]
fn a_deprecated_project_settings_file_is_named_once_by_its_own_path() {
    let dir = unique_temp_dir("bundle-deprecated-settings");
    let template = write_project_template(&dir, "plain", &laid_machine(&[]), "");
    std::fs::remove_file(template.join("settings.json")).expect("a template without settings");
    let project = write_project(&dir, "reports", PLAIN_MANIFEST, Some(&laid_machine(&[])));
    std::fs::create_dir_all(project.join("prompt_templates")).expect("the project's bundle");
    write_fixture_file(&project.join("prompt_templates"), "brief.md", "Brief {subject}.\n");
    std::fs::create_dir_all(project.join(".agents/rhei")).expect("the deprecated home");
    write_fixture_file(&project.join(".agents/rhei"), "settings.json", "{}\n");
    let tmp = dir.join("tmp");

    let dry = rhei_with_tmp(
        &dir,
        &dir,
        &tmp,
        &["instantiate", "plain", "--into", "reports", "--dry-run"],
    );
    assert_success(&dry);
    assert_eq!(
        dry.stderr.matches("which is deprecated").count(),
        1,
        "the deprecated file is named once; got:\n{}",
        dry.stderr
    );
    assert_stderr_contains(&dry, "reports/.agents/rhei/settings.json");
    assert!(
        !dry.stderr.contains(&tmp.display().to_string()),
        "no copy's path reaches the output; got:\n{}",
        dry.stderr
    );
}

/// A replacement that introduces an error of its own is refused naming the
/// project and its `states.yaml`, never the temp copy it was validated in, and
/// so is a project that does not load at all. §FS-rhei-library.2.3
/// §FS-rhei-validate.6
#[test]
fn a_validation_refusal_names_the_project_not_its_copy() {
    let dir = unique_temp_dir("bundle-refusal-paths");
    let timed = |name: &str, timeout: &str| {
        format!(
            "name: {name}\nversion: 1\nstates:\n  work:\n    description: Do the work\n    \
             target: codex:openai:gpt-5.5\n{timeout}  completed:\n    final: true\n    \
             description: Done\ntransitions:\n  - from: work\n    to: completed\n"
        )
    };
    write_project_template(&dir, "untimed", &timed("laidmachine", ""), "");
    let house = timed("housemachine", "    agent_timeout: 1h\n");
    let project = write_project(&dir, "reports", PLAIN_MANIFEST, Some(&house));
    write_fixture_file(
        &project,
        "r.rhei.md",
        "# Rhei: R\n\n## Tasks\n\n### Task one: Runs under the default\n**State:** work\n",
    );
    assert_success(&rhei_in(&dir, &dir, &["validate", "reports"]));
    let before = snapshot(&project);
    let tmp = dir.join("tmp");

    let rebound = rhei_with_tmp(&dir, &dir, &tmp, &["instantiate", "untimed", "--into", "reports"]);
    assert!(!rebound.status.success(), "the replacement is refused; got:\n{}", rebound.stdout);
    assert_stderr_contains(&rebound, "agent_timeout");
    assert_stderr_contains(&rebound, "reports/states.yaml");
    assert!(
        !rebound.stderr.contains(&tmp.display().to_string()),
        "the refusal names the project, not the copy; got:\n{}",
        rebound.stderr
    );
    assert_same_tree(&before, &snapshot(&project), "a refused rebind writes nothing");

    write_fixture_file(&project, "broken.rhei.md", "# Rhei: Broken\n\n## Overview\n\nNo tasks.\n");
    let unloadable =
        rhei_with_tmp(&dir, &dir, &tmp, &["instantiate", "untimed", "--into", "reports"]);
    assert!(!unloadable.status.success(), "got:\n{}", unloadable.stdout);
    assert_stderr_contains(&unloadable, "reports/broken.rhei.md");
    assert!(
        !unloadable.stderr.contains(".tmp"),
        "the parse error names the project's file, not the copy's; got:\n{}",
        unloadable.stderr
    );
}

/// A member that fails to lay is named by the path it would have had, never by
/// the hidden directory it was staged in — laid `--into` a project or with the
/// whole project `--output`. §FS-rhei-templates.6.1.2 §FS-rhei-library.2.2
#[test]
fn a_failed_member_lay_names_the_member_not_its_staging() {
    let dir = unique_temp_dir("bundle-member-staging");
    write_lifecycle_templates(&dir);
    let project = write_project(&dir, "reports", PLAIN_MANIFEST, None);
    write_fixture_file(&project, "intake.rhei.md", "# Rhei: Intake\n\n## Tasks\n");

    let placed = rhei_in(&dir, &dir, &["instantiate", "lifecycle", "--into", "reports"]);
    assert!(!placed.status.success(), "a duplicate member id is refused; got:\n{}", placed.stdout);
    let stderr = placed.stderr.replace('\n', " ");
    assert!(!stderr.contains(".rhei-instantiate-"), "no staging path; got:\n{}", placed.stderr);
    assert!(
        stderr.match_indices("reports/intake").any(|(at, _)| !stderr[at + 14..].starts_with('.')),
        "the member is named by its own directory, reports/intake; got:\n{}",
        placed.stderr
    );

    write_bad_member_template(&dir);
    write_project_template(&dir, "broken", &laid_machine(&[]), "includes:\n  - bad\n");
    let laid = rhei_in(&dir, &dir, &["instantiate", "broken", "--output", "laid"]);
    assert!(!laid.status.success(), "the bad member is refused; got:\n{}", laid.stdout);
    assert_stderr_contains(&laid, "laid/bad");
    assert!(!laid.stderr.contains(".rhei-instantiate-"), "no staging path; got:\n{}", laid.stderr);
}

/// A rebind that fails after writing the root puts back each file's mode as
/// well as its bytes: a copy carries the template's mode. §FS-rhei-library.2.2
#[cfg(unix)]
#[test]
fn a_failed_rebind_puts_back_each_files_mode() {
    use std::os::unix::fs::PermissionsExt;

    let dir = unique_temp_dir("bundle-restore-mode");
    write_bad_member_template(&dir);
    let template =
        write_project_template(&dir, "broken", &laid_machine(&[]), "includes:\n  - bad\n");
    let mode = |path: &Path| std::fs::metadata(path).expect("a mode").permissions().mode() & 0o777;
    let executable = std::fs::Permissions::from_mode(0o755);
    std::fs::set_permissions(template.join("scripts/collect.sh"), executable).expect("chmod");
    let project = write_project(&dir, "reports", PLAIN_MANIFEST, Some(&laid_machine(&[])));
    for bundle in ["prompt_templates", "scripts"] {
        std::fs::create_dir_all(project.join(bundle)).expect("the project's bundle");
    }
    write_fixture_file(&project.join("prompt_templates"), "brief.md", "Brief {subject}.\n");
    let script = write_fixture_file(&project.join("scripts"), "collect.sh", "echo mine\n");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o644)).expect("chmod");
    let before = snapshot(&project);

    let rebound = rhei_in(&dir, &dir, &["instantiate", "broken", "--into", "reports"]);
    assert!(!rebound.status.success(), "the bad member fails the rebind; got:\n{}", rebound.stdout);
    assert_stderr_contains(&rebound, "nosuchstate");
    assert_same_tree(&before, &snapshot(&project), "a failed rebind puts every byte back");
    assert_eq!(mode(&script), 0o644, "and every mode");
}
