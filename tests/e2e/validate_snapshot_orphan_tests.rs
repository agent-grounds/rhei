//! Named-member orphan presentation keeps whole-project validation intact.
//! §FS-rhei-validate.4.7

use std::fs;

use super::validate_snapshot_orphan_support::*;
use super::*;

#[test]
#[ignore = "red until #774: combine other members' orphan warnings; implement removes this"]
fn validate_snapshot_orphans_named_member_summarizes_other_owners() {
    let fixture = Fixture::new("rhei-774-orphan-summary");
    // Premises before the red assertion: readable manifests, genuine orphans,
    // both emission origins, no live false positives, and a valid merged graph.
    let rows = fixture.orphan_rows(true);
    assert_eq!(rows.len(), 7);
    assert_eq!(
        fixture.orphan_rows(false).len(),
        6,
        "the default listing omits the operator record"
    );
    assert_eq!(rows.iter().filter(|row| row["produced_by"] == "operator").count(), 1);
    let whole = fixture.validate(&fixture.project, &[]);
    assert_eq!(whole.lines().filter(|line| line.starts_with("warning: snapshot ")).count(), 7);
    for reference in std::iter::once(&fixture.own).chain(&fixture.others) {
        assert_eq!(whole.matches(reference).count(), 1, "whole-project detail: {whole}");
    }
    let named = fixture.validate(&fixture.project, &["csa-models"]);
    assert_own_detail(&named, &fixture.own);

    // Naming one rhei must not hide a real resolution error in another.
    let task = fixture.project.join("csa-profile/tasks/measure.md");
    let original = fs::read_to_string(&task).unwrap();
    fs::write(&task, format!("{original}**Prior:** csa-fix.missing\n")).unwrap();
    let refused = fixture.case.rhei(&fixture.project, &["validate", "csa-models"]);
    assert_eq!(refused.status.code(), Some(1), "other-member error must refuse validation");
    let diagnostic = stderr(&refused);
    assert!(
        diagnostic.contains("csa-profile.measure") && diagnostic.contains("csa-fix.missing"),
        "the other member's unresolved prior remains visible: {diagnostic}"
    );
    fs::write(task, original).unwrap();

    // Removing the other owners' cache records leaves own detail and no summary.
    let cache = fixture.project.join(".rhei/cache/snapshots");
    for task in ["csa-fix.deliver", "csa-fix.hotfix", "csa-profile.baseline", "csa-profile.measure"]
    {
        fs::remove_dir_all(cache.join(task)).unwrap();
    }
    let own_only = fixture.validate(&fixture.project, &["csa-models"]);
    assert_own_detail(&own_only, &fixture.own);
    assert_eq!(
        own_only
            .lines()
            .filter(|line| line.starts_with("warning:") && line.contains("orphan"))
            .count(),
        1,
        "no summary for an empty other-member set: {own_only}"
    );
    // The same cache detail remains appropriate for a standalone workspace.
    let member = fixture.project.join("csa-models");
    fs::create_dir_all(member.join(".rhei/cache")).unwrap();
    fs::rename(&cache, member.join(".rhei/cache/snapshots")).unwrap();
    fs::remove_file(fixture.project.join("index.panta.md")).unwrap();
    let standalone = fixture.validate(&member, &[]);
    assert_own_detail(&standalone, &fixture.own);
    assert_eq!(standalone.lines().filter(|line| line.starts_with("warning: snapshot ")).count(), 1);
    fs::remove_dir_all(member.join(".rhei/cache/snapshots")).unwrap();
    let empty = fixture.validate(&fixture.project, &["csa-models"]);
    assert!(!empty.contains("orphan"), "no orphan diagnostic for an empty cache: {empty}");

    // The issue's red assertion judges the captured full-cache output only
    // after the fixture and preserved whole-project behavior are established.
    let combined = summary(&named);
    assert!(
        named.find(&fixture.own).unwrap() < named.find(combined).unwrap(),
        "own detail must precede the other-member summary: {named}"
    );
    for reference in &fixture.others {
        assert!(!named.contains(reference), "other member detail leaked: {named}");
    }
}

#[test]
#[ignore = "red until #774: preserve member orphan summaries on each watch pass; implement removes this"]
fn validate_snapshot_orphans_watch_repeats_member_summary() {
    use super::provider_limit_support::{wait_for, RunningChild};

    let fixture = Fixture::new("rhei-774-orphan-watch");
    let mut cmd = rhei_command(&fixture.case.home);
    cmd.current_dir(&fixture.project).args(["validate", "csa-models", "--watch"]);
    let mut child = RunningChild::spawn(&mut cmd, &fixture.case.root, "watch");
    let output = fixture.case.root.join("watch.stdout");
    // Both the old final detail and the new final summary complete a report;
    // absence of the desired summary must cause an assertion, never a timeout.
    let complete = |said: &str| {
        said.contains("Validation succeeded")
            && said.contains(&fixture.own)
            && (said.contains(fixture.others.last().unwrap())
                || said.contains("rhei snapshot list"))
    };
    wait_for("complete initial validation pass", &mut child, || {
        complete(&stdout_from_file(&output))
    });
    let task = fixture.project.join("csa-models/tasks/train.md");
    let original = fs::read_to_string(&task).unwrap();
    fs::write(task, format!("{original}\nA genuine watched input edit.\n")).unwrap();
    let marker = "--- change detected, revalidating ---";
    wait_for("complete validation after task edit", &mut child, || {
        stdout_from_file(&output).rsplit_once(marker).is_some_and(|(_, pass)| complete(pass))
    });
    child.stop();
    let said = stdout_from_file(&output);
    let passes: Vec<_> = said.split(marker).collect();
    assert!(passes.len() >= 2, "watch must revalidate the edited input: {said}");
    for pass in passes.iter().filter(|pass| pass.contains("Validation succeeded")) {
        assert_own_detail(pass, &fixture.own);
        summary(pass);
    }
}

#[test]
#[cfg(unix)]
#[ignore = "red until #774: offer an all-origin inspection command for the same project; implement removes this"]
fn validate_snapshot_orphans_member_paths_offer_working_project_hint() {
    use super::help_target_tests::pasted;

    let fixture = Fixture::new("rhei-774-orphan-hint");
    let member = fixture.project.join("csa-models");
    let index = member.join("index.rhei.md");
    let outside = fixture.case.root.as_ref();
    for (cwd, target) in [
        (outside, vec![member.to_str().unwrap()]),
        (outside, vec![index.to_str().unwrap()]),
        (member.as_path(), vec![]),
    ] {
        let said = fixture.validate(cwd, &target);
        assert_own_detail(&said, &fixture.own);
        let line = summary(&said);
        // Inline backticks delimit the copyable command, independently of prose.
        let hint = line
            .split('`')
            .find(|part| part.starts_with("rhei snapshot list"))
            .expect("copyable snapshot list command");
        let listed = pasted(hint, cwd, &fixture.case.home);
        assert!(
            listed.status.success(),
            "hint must work from caller cwd: {hint}\n{}",
            listed.stderr
        );
        let rows: Vec<_> = listed.stdout.lines().filter(|row| row.contains("\t_state\t")).collect();
        assert_eq!(
            rows.len(),
            7,
            "the hinted command must list all seven orphans: {}",
            listed.stdout
        );
        assert_eq!(rows.iter().filter(|row| row.ends_with("\toperator")).count(), 1);
        for reference in std::iter::once(&fixture.own).chain(&fixture.others) {
            let (task, rest) = reference.split_once(":_state:").unwrap();
            let (state, rest) = rest.split_once('@').unwrap();
            let (visit, target) = rest.split_once(':').unwrap();
            let slug = target.strip_suffix("/g1").unwrap();
            let identity = format!("{task}\t_state\t{state}\t{visit}\t{slug}\tg1\t");
            assert!(
                listed.stdout.contains(&identity),
                "hint omitted {reference}: {}",
                listed.stdout
            );
        }
    }
}
