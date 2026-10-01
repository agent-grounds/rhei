use std::fs;
use std::path::{Path, PathBuf};

use super::{rhei_binary, rhei_process_at, unique_temp_dir, CliRun};
#[cfg(unix)]
use super::{rhei_process, stderr};

/// Run `rhei install-skills` with a fake HOME and optional extra args.
fn run_install_skills(home: &Path, extra_args: &[&str]) -> CliRun {
    let mut cmd = super::rhei_command(home);
    cmd.arg("install-skills");
    for arg in extra_args {
        cmd.arg(arg);
    }
    let output = cmd.output().expect("rhei command should run");
    CliRun::from(&output)
}

/// Run `rhei install-skills` from a specific working directory (for --local).
fn run_install_skills_in_dir(home: &Path, cwd: &Path, extra_args: &[&str]) -> CliRun {
    let mut cmd = super::rhei_command(home);
    cmd.current_dir(cwd);
    cmd.arg("install-skills");
    for arg in extra_args {
        cmd.arg(arg);
    }
    let output = cmd.output().expect("rhei command should run");
    CliRun::from(&output)
}

/// Run a specific `rhei` binary from a specific working directory.
fn run_install_skills_with(home: &Path, bin: &Path, cwd: &Path, extra_args: &[&str]) -> CliRun {
    let mut cmd = rhei_process_at(bin);
    cmd.env("HOME", home);
    cmd.env("XDG_STATE_HOME", home.join("state"));
    cmd.current_dir(cwd);
    cmd.arg("install-skills");
    for arg in extra_args {
        cmd.arg(arg);
    }
    // Copying an executable in one test thread while another thread forks
    // leaves the child holding the write descriptor across its exec, so a fresh
    // copy can be ETXTBSY for as long as that child takes to start. It is an
    // artifact of running these tests in parallel, not of the code under test.
    let mut attempt = 0;
    let output = loop {
        match cmd.output() {
            Ok(output) => break output,
            // ETXTBSY by number: `ErrorKind::ExecutableFileBusy` is still unstable.
            Err(err) if err.raw_os_error() == Some(26) && attempt < 100 => {
                attempt += 1;
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(err) => panic!("rhei command should run (attempt {attempt}): {err}"),
        }
    };
    CliRun::from(&output)
}

/// Copy the built binary somewhere with no checkout above it and no packaged
/// asset directory beside it — the shape of a `cargo install`ed `rhei`, which
/// is what the embedded skills exist for. §FS-rhei-install-skills.4.3
fn binary_outside_checkout(dir: &Path) -> PathBuf {
    let dest = dir.join("rhei");
    fs::copy(rhei_binary(), &dest).expect("copy the rhei binary");
    dest
}

/// `install-skills` used to resolve skills by walking up from the binary, so a
/// `cargo install`ed `rhei` with no repo above it could not find them — even
/// when run inside a checkout. §FS-rhei-install-skills.4.3
#[test]
fn installs_from_a_binary_outside_any_checkout() {
    let home = unique_temp_dir("install-embedded-home");
    let bin_dir = unique_temp_dir("install-embedded-bin");
    let cwd = unique_temp_dir("install-embedded-cwd");
    let bin = binary_outside_checkout(&bin_dir);

    let result = run_install_skills_with(&home, &bin, &cwd, &["--agent", "claude-code"]);
    assert!(
        result.status.success(),
        "install should succeed with no checkout in sight\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    // Every embedded skill, including the nested `references/` payload that a
    // single-file extraction would drop.
    for skill in [
        "rhei-plan-writer",
        "rhei-plan-worker",
        "rhei-state-machine-writer",
        "rhei-template-writer",
    ] {
        assert!(
            home.join(".claude/skills").join(skill).join("SKILL.md").exists(),
            "{skill} should be installed from the embedded copy"
        );
    }
    assert!(home.join(".claude/skills/rhei-plan-writer/references/default-states.md").exists());
}

/// The shape rule's extract ships under every skill that authors a plan or a
/// machine, as three real files with one content, from the binary's embedded
/// copy as well as from a checkout — a symlink would not survive embedding.
/// §FS-rhei-install-skills.4.8 §FS-rhei-shape.7
#[test]
fn the_shape_extract_ships_under_every_authoring_skill() {
    let source = fs::read_to_string(
        super::repo_root().join("crates/rhei-cli/skills/rhei-plan-writer/references/shape.md"),
    )
    .expect("the checkout carries the extract");
    let bin_dir = unique_temp_dir("install-shape-bin");
    let cwd = unique_temp_dir("install-shape-cwd");
    let embedded = binary_outside_checkout(&bin_dir);
    for (label, bin) in [("embedded", embedded), ("checkout", rhei_binary())] {
        let home = unique_temp_dir(&format!("install-shape-{label}"));
        let result = run_install_skills_with(&home, &bin, &cwd, &["--agent", "claude-code"]);
        assert!(result.status.success(), "{label}: install failed\n{}", result.stderr);
        for skill in ["rhei-plan-writer", "rhei-state-machine-writer", "rhei-template-writer"] {
            let path = home.join(".claude/skills").join(skill).join("references/shape.md");
            let installed = fs::read_to_string(&path)
                .unwrap_or_else(|err| panic!("{label}: {} is installed: {err}", path.display()));
            assert_eq!(installed, source, "{label}: {skill} ships the one extract");
        }
    }
}

/// The extraction is temporary, so a symlink into it would dangle. Say that
/// instead of installing something broken. §FS-rhei-install-skills.4.4
#[test]
fn link_outside_a_checkout_explains_it_needs_files_on_disk() {
    let home = unique_temp_dir("install-embedded-link-home");
    let bin_dir = unique_temp_dir("install-embedded-link-bin");
    let cwd = unique_temp_dir("install-embedded-link-cwd");
    let bin = binary_outside_checkout(&bin_dir);

    let result = run_install_skills_with(&home, &bin, &cwd, &["--agent", "kilocode", "--link"]);

    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(combined.contains("--link"), "error should name the flag\n{combined}");
    assert!(
        combined.contains("crates/rhei-cli/skills/"),
        "error should name the path that would have satisfied it\n{combined}"
    );
    assert!(
        !home.join(".kilocode/rules/rhei-plan-writer.md").exists(),
        "nothing should be installed when --link cannot be honored"
    );
}

/// A misspelled `--skills` value should say what the binary actually carries.
/// §FS-rhei-install-skills.4.3
#[test]
fn unknown_skill_name_lists_the_embedded_skills() {
    let home = unique_temp_dir("install-unknown-skill-home");
    let bin_dir = unique_temp_dir("install-unknown-skill-bin");
    let cwd = unique_temp_dir("install-unknown-skill-cwd");
    let bin = binary_outside_checkout(&bin_dir);

    let result = run_install_skills_with(
        &home,
        &bin,
        &cwd,
        &["--agent", "claude-code", "--skills", "rhei-plan-writr"],
    );

    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(combined.contains("rhei-plan-writr"), "error should quote the bad name\n{combined}");
    assert!(
        combined.contains("rhei-plan-writer"),
        "error should list the skills that do exist\n{combined}"
    );
}

/// A checkout's skills win over the binary's embedded copy, so editing a skill
/// and installing it does not silently install the build's stale version.
/// §FS-rhei-install-skills.4.3
#[test]
fn a_checkout_overrides_the_embedded_copy() {
    let home = unique_temp_dir("install-checkout-override-home");
    let checkout = unique_temp_dir("install-checkout-override-repo");
    let bin_dir = unique_temp_dir("install-checkout-override-bin");
    let bin = binary_outside_checkout(&bin_dir);

    let skill_dir = checkout.join("crates/rhei-cli/skills/rhei-plan-writer");
    fs::create_dir_all(&skill_dir).expect("create checkout skill dir");
    fs::write(skill_dir.join("SKILL.md"), "edited in the checkout\n").expect("write skill");

    let result = run_install_skills_with(
        &home,
        &bin,
        &checkout,
        &["--agent", "claude-code", "--skills", "rhei-plan-writer"],
    );
    assert!(
        result.status.success(),
        "install should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    let installed = fs::read_to_string(home.join(".claude/skills/rhei-plan-writer/SKILL.md"))
        .expect("read installed skill");
    assert!(
        installed.contains("edited in the checkout"),
        "the checkout copy should win over the embedded one, got:\n{installed}"
    );
}

/// A `CLAUDE.md` holding a rhei block, then other content before the next
/// heading. Reinstalling used to delete everything up to that heading.
const SHARED_CLAUDE_MD: &str = "\
# Global instructions

Keep this paragraph.

# rhei
- **rhei-plan-writer** (`~/.claude/skills/rhei-plan-writer/SKILL.md`) — old. Trigger: `/rhei-plan-writer`
When the user types `/rhei-plan-writer`, invoke the Skill tool.

<!-- >>> another tool's block >>> -->
## Another Tool

Text that belongs to another tool.
<!-- <<< another tool's block <<< -->
";

/// Updating the registration replaced everything from `# rhei` to the next
/// heading, so a marker block or paragraph sitting between the two was deleted
/// from the user's `CLAUDE.md`. §FS-rhei-install-skills.4.5
#[test]
fn reinstall_keeps_what_follows_the_rhei_block() {
    let home = unique_temp_dir("install-preserve-following");
    fs::create_dir_all(home.join(".claude")).expect("create .claude");
    fs::write(home.join(".claude/CLAUDE.md"), SHARED_CLAUDE_MD).expect("seed CLAUDE.md");

    let result = run_install_skills(&home, &["--agent", "claude-code"]);
    assert!(
        result.status.success(),
        "install should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    let updated = fs::read_to_string(home.join(".claude/CLAUDE.md")).expect("read CLAUDE.md");
    assert!(
        updated.contains("<!-- >>> another tool's block >>> -->"),
        "the following block's opening marker must survive, got:\n{updated}"
    );
    assert!(updated.contains("Text that belongs to another tool."), "got:\n{updated}");
    assert!(updated.contains("Keep this paragraph."), "got:\n{updated}");

    // The block itself is refreshed rather than duplicated.
    assert!(updated.contains("rhei-template-writer"), "got:\n{updated}");
    assert!(!updated.contains("— old."), "the stale block should be gone, got:\n{updated}");
    assert_eq!(updated.matches("# rhei\n").count(), 1, "got:\n{updated}");
}

/// Uninstall took the same over-wide range as the update path.
/// §FS-rhei-install-skills.4.5
#[test]
fn uninstall_keeps_what_follows_the_rhei_block() {
    let home = unique_temp_dir("install-preserve-following-uninstall");
    fs::create_dir_all(home.join(".claude")).expect("create .claude");
    fs::write(home.join(".claude/CLAUDE.md"), SHARED_CLAUDE_MD).expect("seed CLAUDE.md");

    let result = run_install_skills(&home, &["--agent", "claude-code", "--uninstall"]);
    assert!(
        result.status.success(),
        "uninstall should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    let updated = fs::read_to_string(home.join(".claude/CLAUDE.md")).expect("read CLAUDE.md");
    assert!(
        updated.contains("<!-- >>> another tool's block >>> -->"),
        "the following block's opening marker must survive, got:\n{updated}"
    );
    assert!(updated.contains("Keep this paragraph."), "got:\n{updated}");
    assert!(!updated.contains("# rhei"), "the rhei block should be gone, got:\n{updated}");
    assert!(!updated.contains("rhei-plan-writer"), "got:\n{updated}");
}

#[test]
fn global_install_copy_claude_code() {
    let home = unique_temp_dir("install-claude-code");

    let result = run_install_skills(&home, &["--agent", "claude-code"]);
    assert!(
        result.status.success(),
        "install should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    // Verify skill directories were copied.
    assert!(home.join(".claude/skills/rhei-plan-writer/SKILL.md").exists());
    assert!(home.join(".claude/skills/rhei-plan-worker/SKILL.md").exists());
    assert!(home.join(".claude/skills/rhei-state-machine-writer/SKILL.md").exists());
    assert!(home.join(".claude/skills/rhei-template-writer/SKILL.md").exists());

    // Verify CLAUDE.md has registration block.
    let claude_md = fs::read_to_string(home.join(".claude/CLAUDE.md")).expect("read CLAUDE.md");
    assert!(claude_md.contains("# rhei"));
    assert!(claude_md.contains("rhei-plan-writer"));
    assert!(claude_md.contains("rhei-plan-worker"));
    assert!(claude_md.contains("rhei-state-machine-writer"));
    assert!(claude_md.contains("rhei-template-writer"));

    // Verify output format.
    assert!(result.stdout.contains("claude-code:"));
    assert!(result.stdout.contains("✓"));
    assert!(result.stdout.contains("registered 4 skills"));
    assert!(result.stdout.contains("Installed rhei skills for 1 agent."));
}

#[test]
fn local_install_cursor() {
    let home = unique_temp_dir("install-cursor-local");
    let project = unique_temp_dir("install-cursor-project");
    // Create a project marker so find_project_root works.
    fs::write(project.join("Cargo.toml"), "[package]\nname = \"test\"").expect("write marker");

    let result = run_install_skills_in_dir(&home, &project, &["--local", "--agent", "cursor"]);
    assert!(
        result.status.success(),
        "install should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    // Verify .mdc files were created in the project.
    assert!(project.join(".cursor/rules/rhei-plan-writer.mdc").exists());
    assert!(project.join(".cursor/rules/rhei-plan-worker.mdc").exists());
    assert!(project.join(".cursor/rules/rhei-state-machine-writer.mdc").exists());

    // Verify MDC format.
    let mdc =
        fs::read_to_string(project.join(".cursor/rules/rhei-plan-writer.mdc")).expect("read mdc");
    assert!(mdc.starts_with("---\n"));
    assert!(mdc.contains("alwaysApply: false"));
}

/// `--link` is the one install mode with a platform in it: it makes symlinks,
/// which an unprivileged Windows process cannot. Both halves are pinned here —
/// the link where links exist, and the refusal, with its help, where they do
/// not.
#[test]
fn link_mode_creates_symlinks() {
    let home = unique_temp_dir("install-link");

    let result = run_install_skills(&home, &["--agent", "kilocode", "--link"]);
    assert!(
        result.status.success(),
        "install should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    let skill_path = home.join(".kilocode").join("rules").join("rhei-plan-writer.md");
    if cfg!(unix) {
        assert!(skill_path.exists(), "skill file should exist");
        assert!(
            skill_path.symlink_metadata().unwrap().file_type().is_symlink(),
            "should be a symlink"
        );
    } else {
        assert!(!skill_path.exists(), "a refused link installs nothing: {}", result.stdout);
        assert!(
            result.stderr.contains("symlinks are only supported on Unix platforms"),
            "the refusal names the platform limit; got:\n{}",
            result.stderr
        );
        // And the way through it: a refusal with no next action is half a
        // diagnostic. §FS-rhei-errors.1.2
        assert!(
            result.stderr.contains("drop --link and rhei will copy the skill files instead"),
            "the refusal names plain copying as the way through; got:\n{}",
            result.stderr
        );
    }
}

/// Every per-agent failure this command reports goes through one `eprintln!`,
/// and it used to print the message without the `help =`. Windows reaches it
/// through `--link`; this reaches the same line on the platform that runs the
/// whole suite, through an install with nowhere to install to.
// §FS-rhei-errors.1.2

// Unix: it unsets `HOME`, where a user-level install goes on Unix; Windows
// derives that directory from `USERPROFILE`, so the same removal is not the
// same experiment there. §REQ-cross-platform.3
#[cfg(unix)]
#[test]
fn a_refused_install_prints_its_help_and_not_only_its_message() {
    let mut cmd = rhei_process();
    cmd.env_remove("HOME");
    cmd.args(["install-skills", "--agent", "kilocode", "--link"]);
    let output = cmd.output().expect("rhei command should run");
    let stderr = stderr(&output);

    assert!(
        stderr.contains("HOME environment variable not set"),
        "the message names what is missing; got:\n{stderr}"
    );
    assert!(
        stderr.contains("rhei writes user-level files under $HOME"),
        "and the help says what to do about it; got:\n{stderr}"
    );
}

#[test]
fn global_install_copy_codex() {
    let home = unique_temp_dir("install-codex");

    let result = run_install_skills(&home, &["--agent", "codex"]);
    assert!(
        result.status.success(),
        "install should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    assert!(home.join(".agents/skills/rhei-plan-writer/SKILL.md").exists());
    assert!(home.join(".agents/skills/rhei-plan-worker/SKILL.md").exists());
    assert!(home.join(".agents/skills/rhei-state-machine-writer/SKILL.md").exists());
    assert!(!home.join(".codex/instructions.md").exists());

    assert!(result.stdout.contains("codex:"));
    // The reported destination is a real path, so it is spelled the host's way.
    let listed = Path::new(".agents").join("skills").join("rhei-plan-writer");
    assert!(
        result.stdout.contains(&listed.display().to_string()),
        "the report names each skill it wrote; got:\n{}",
        result.stdout
    );
    assert!(result.stdout.contains("Installed rhei skills for 1 agent."));
}

#[test]
fn uninstall_removes_files() {
    let home = unique_temp_dir("install-uninstall");

    // First install.
    let result = run_install_skills(&home, &["--agent", "claude-code"]);
    assert!(result.status.success());
    assert!(home.join(".claude/skills/rhei-plan-writer/SKILL.md").exists());

    // Then uninstall.
    let result = run_install_skills(&home, &["--agent", "claude-code", "--uninstall"]);
    assert!(
        result.status.success(),
        "uninstall should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    // Skill directories should be removed.
    assert!(!home.join(".claude/skills/rhei-plan-writer").exists());
    assert!(!home.join(".claude/skills/rhei-plan-worker").exists());
    assert!(!home.join(".claude/skills/rhei-state-machine-writer").exists());

    assert!(result.stdout.contains("Uninstalled"));
}

#[test]
fn dry_run_does_not_create_files() {
    let home = unique_temp_dir("install-dryrun");

    let result = run_install_skills(&home, &["--agent", "claude-code", "--dry-run"]);
    assert!(
        result.status.success(),
        "dry-run should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    // Output should mention dry-run.
    assert!(result.stdout.contains("[dry-run]"));

    // No files should have been created (except the CLAUDE.md registration output line).
    assert!(!home.join(".claude/skills/rhei-plan-writer").exists());
}

#[test]
fn reinstall_overwrites_existing_skill_files() {
    let home = unique_temp_dir("install-idempotent");

    // First install.
    let result = run_install_skills(&home, &["--agent", "claude-code"]);
    assert!(result.status.success());

    let installed_skill = home.join(".claude/skills/rhei-plan-writer/SKILL.md");
    fs::write(&installed_skill, "stale test content\n").expect("overwrite installed skill");

    // Second install should refresh the installed content.
    let result = run_install_skills(&home, &["--agent", "claude-code"]);
    assert!(result.status.success());
    let refreshed = fs::read_to_string(&installed_skill).expect("read refreshed skill");
    assert!(
        !refreshed.contains("stale test content"),
        "second install should overwrite stale content"
    );
    let listed = Path::new(".claude").join("skills").join("rhei-plan-writer");
    assert!(
        result.stdout.contains(&listed.display().to_string()),
        "second install should rewrite skills\nstdout:\n{}",
        result.stdout
    );
}
