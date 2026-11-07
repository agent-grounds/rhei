// Which lines a restore takes for a region's boundaries when the plan's text
// quotes them: a copy of the heading in a frontmatter value or a fence is not
// the heading, and a fence the worker left open in its own region does not
// hide the heading that follows it.

// §AR-source-file-size.3 §FS-rhei-run.3.7.3 §FS-rhei-plan-language.2.1

mod worker_restore_boundaries {
    use super::super::*;
    use rhei_core::header::DocumentRole::Plan;

    const TASKS: &str = "\
## Tasks

### Task 1: Earlier
**State:** completed

Earlier work, must survive.

### Task 2: Cover
**State:** work

Body.

### Task 3: After
**State:** completed

After.
";

    const HEADING: &str = "### Task 2: Cover\n**State:** work\n";

    /// A plan whose frontmatter value spells Task 2's heading at column 0.
    fn value_copy() -> String {
        format!("# Rhei: P\n\n---\nmetadata:\n  note: \"\n### Task 2: Cover\n\"\n---\n\n{TASKS}")
    }

    /// A plan quoting Task 2's heading in a fenced block above the chapter.
    fn fenced_copy() -> String {
        format!("# Rhei: P\n\n## Overview\n\n```\n### Task 2: Cover\n```\n\n{TASKS}")
    }

    /// The worker's bad edit: a heading in its body no grammar admits.
    fn appended(text: &str) -> String {
        text.replace(HEADING, &format!("{HEADING}\n#### Visit 1 (cover)\n\nnote\n"))
    }

    /// The same edit, with the worker's own heading renamed as well.
    fn renamed(text: &str) -> String {
        appended(text).replace("### Task 2: Cover\n", "### Task 2: Cover renamed\n")
    }

    /// A copy above the region is not the heading, so a rename leaves nothing to
    /// find and the restore refuses, writing nothing. §FS-rhei-run.3.7.3
    #[test]
    fn a_heading_copied_in_a_value_or_a_fence_does_not_stand_for_a_renamed_heading() {
        for plan in [value_copy(), fenced_copy()] {
            let snapshot = snapshot_region(&plan, Plan, "2").expect("Task 2 is declared once");
            let broken = renamed(&plan);
            assert_eq!(region_span(&broken, &snapshot), Err(RestoreRefusal::Heading { found: 0 }));
        }
    }

    /// A copy above the region does not double the heading, so an ordinary bad
    /// edit is put back and the text around it kept. §FS-rhei-run.3.7.3
    #[test]
    fn a_heading_copied_in_a_value_does_not_block_the_restore() {
        let plan = value_copy();
        let snapshot = snapshot_region(&plan, Plan, "2").expect("Task 2 is declared once");
        let (restored, replaced) = restore_region(&appended(&plan), &snapshot).expect("restore");
        assert_eq!(restored, plan);
        assert!(replaced.contains("#### Visit 1 (cover)"));
    }

    /// The fence a worker leaves open in its own region does not quote the
    /// heading that followed it, so the restore still finds it. §FS-rhei-run.3.7.3
    #[test]
    fn a_fence_left_open_in_the_region_does_not_hide_the_following_heading() {
        let plan = format!("# Rhei: P\n\n{TASKS}");
        let snapshot = snapshot_region(&plan, Plan, "2").expect("Task 2");
        assert_eq!(snapshot.following.as_deref(), Some("### Task 3: After"));
        let broken = plan.replace("Body.\n", "Body.\n\n```\nunclosed\n");
        let (restored, replaced) = restore_region(&broken, &snapshot).expect("restore");
        assert_eq!(restored, plan);
        assert!(replaced.contains("unclosed"));
    }

    /// Two copies of the following heading below the task still refuse.
    /// §FS-rhei-run.3.7.3
    #[test]
    fn a_following_heading_found_twice_below_still_refuses() {
        let plan = format!("# Rhei: P\n\n{TASKS}");
        let snapshot = snapshot_region(&plan, Plan, "2").expect("Task 2");
        let twice = plan.replace("Body.\n", "Body.\n\n```\n### Task 3: After\n");
        assert_eq!(region_span(&twice, &snapshot), Err(RestoreRefusal::Following { found: 2 }));
    }

    /// A plan with a copy of Task 3's heading in a frontmatter value above the
    /// chapter and another in a fence under a later Task 4.
    fn following_copied_above_and_below() -> String {
        let later = "\n### Task 4: Later\n**State:** completed\n\nQuoting:\n\n```\n### Task 3: After\n```\n\nTail.\n";
        format!("# Rhei: P\n\n---\nmetadata:\n  note: \"\n### Task 3: After\n\"\n---\n\n{TASKS}{later}")
    }

    /// The following heading is counted over the whole file, so a copy above the
    /// task is a second copy: when the worker renamed the heading, the fenced copy
    /// below is not taken for it, and Task 3 and Task 4 are kept. §FS-rhei-run.3.7.3
    #[test]
    fn a_following_heading_copied_above_and_quoted_below_refuses_after_a_rename() {
        let plan = following_copied_above_and_below();
        let snapshot = snapshot_region(&plan, Plan, "2").expect("Task 2");
        assert_eq!(snapshot.following.as_deref(), Some("### Task 3: After"));
        let broken = appended(&plan).replace("\n### Task 3: After\n**State:**", "\n### Task 3: After, renamed\n**State:**");
        assert_eq!(region_span(&broken, &snapshot), Err(RestoreRefusal::Following { found: 2 }));
    }

    /// A copy above the task is no boundary, but it is still a second copy of the
    /// line, so even an ordinary bad edit is refused rather than guessed at.
    /// §FS-rhei-run.3.7.3
    #[test]
    fn a_following_heading_copied_in_a_value_above_refuses() {
        let plan = format!("# Rhei: P\n\n---\nmetadata:\n  note: \"\n### Task 3: After\n\"\n---\n\n{TASKS}");
        let snapshot = snapshot_region(&plan, Plan, "2").expect("Task 2");
        assert_eq!(region_span(&appended(&plan), &snapshot), Err(RestoreRefusal::Following { found: 2 }));
    }

    /// The one occurrence of the following line above the task's heading is not
    /// below it, so the restore refuses rather than taking it. §FS-rhei-run.3.7.3
    #[test]
    fn a_following_heading_found_once_above_the_task_refuses() {
        let plan = format!("# Rhei: P\n\n{TASKS}");
        let snapshot = snapshot_region(&plan, Plan, "2").expect("Task 2");
        let moved = appended(&plan)
            .replace("\n### Task 3: After\n**State:**", "\n### Task 3: After, renamed\n**State:**")
            .replace("Earlier work, must survive.\n", "Earlier work, must survive.\n\n```\n### Task 3: After\n```\n");
        assert_eq!(region_span(&moved, &snapshot), Err(RestoreRefusal::Following { found: 1 }));
    }
}
