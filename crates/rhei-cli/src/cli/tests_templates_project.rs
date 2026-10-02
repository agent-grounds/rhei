// Laying a Panta project, tested where each decision is made: the layout, what
// a `--into` names, and which tickets a replacement default answers for.
// §FS-rhei-templates.2 §FS-rhei-library.2.2 §FS-rhei-library.2.3
mod templates_project_tests {
    use super::super::*;

    /// A machine with one working state and the two terminals, routing `task`
    /// and every kind in `kinds` through one profile.
    fn machine(name: &str, working: &str, kinds: &[&str]) -> String {
        let mut by_type = String::from("    task: profile\n");
        for kind in kinds {
            by_type.push_str(&format!("    {kind}: profile\n"));
        }
        format!(
            "name: {name}\nversion: 1\nstates:\n  {working}:\n    description: Work\n    \
             instructions: |\n      Do the work.\n  completed:\n    final: true\n    description: Done\n  \
             cancelled:\n    final: true\n    description: Abandoned\ntransitions:\n  - from: {working}\n    \
             to: completed\n  - from: \"*\"\n    to: cancelled\nprofiles:\n  profile:\n    initial: {working}\n    \
             allowed: [{working}, completed, cancelled]\nnode_policy:\n  root: profile\n  default: profile\n  \
             by_type:\n{by_type}"
        )
    }

    fn write(root: &Path, relative: &str, text: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn ticket(state: &str) -> String {
        format!("### Task 1: A ticket\n**State:** {state}\n")
    }

    // ---- 1. layout detection -----------------------------------------------

    /// Each entry point alone names its layout, and `index.panta.md` is the
    /// third. §FS-rhei-templates.2
    #[test]
    fn each_entry_point_alone_names_its_layout() {
        let dir = tempfile::tempdir().unwrap();
        for (entry, layout) in [
            ("plan.rhei.md", TemplateLayout::SingleFile),
            ("index.rhei.md", TemplateLayout::Workspace),
            ("index.panta.md", TemplateLayout::Project),
        ] {
            let template = dir.path().join(entry.replace('.', "-"));
            write(&template, entry, "---\nname: t\n---\n");
            fs::create_dir_all(template.join("tasks")).unwrap();
            let detected = detect_template_layout(&template).expect("one entry point is a layout");
            assert_eq!(detected, layout, "{entry}");
            assert_eq!(detected.as_str(), ["single-file", "workspace", "project"][layout as usize]);
        }
    }

    /// No entry point, and two, are both refused: a template has exactly one.
    /// §FS-rhei-templates.2
    #[test]
    fn none_or_two_entry_points_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let none = detect_template_layout(dir.path()).expect_err("no entry point").to_string();
        assert!(none.contains("index.panta.md"), "names the project entry point; got: {none}");

        write(dir.path(), "plan.rhei.md", "");
        write(dir.path(), "index.panta.md", "");
        let two = detect_template_layout(dir.path()).expect_err("two entry points").to_string();
        assert!(
            two.contains("plan.rhei.md") && two.contains("index.panta.md"),
            "names both; got: {two}"
        );
    }

    // ---- 2. what a `--into` names ------------------------------------------

    fn project_in(dir: &Path, name: &str) -> PathBuf {
        write(&dir.join(name), "index.panta.md", "# Panta: P\n");
        dir.join(name)
    }

    /// A bare id splits at its first dot into a rhei and a task; a directory
    /// with `index.panta.md` is a project, whether named bare or as `.`.
    /// §FS-rhei-library.2.2
    #[test]
    fn a_bare_id_is_split_and_a_project_is_found_by_its_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let project = project_in(dir.path(), "reports");
        write(&project, "own/index.rhei.md", "# Rhei: Own\n");

        match resolve_into_target_from(&project, "own.3").unwrap() {
            IntoTarget::Rhei(host) => {
                assert_eq!(host.root, project.join("own"));
                assert_eq!(host.parent.as_deref(), Some("3"));
                assert_eq!(host.project.as_deref(), Some(project.as_path()));
            }
            IntoTarget::Project(_) => panic!("`own.3` names a task in a rhei"),
        }
        for (cwd, target) in [(dir.path(), "reports"), (project.as_path(), ".")] {
            match resolve_into_target_from(cwd, target).unwrap() {
                IntoTarget::Project(found) => assert_eq!(found, project, "{target}"),
                IntoTarget::Rhei(host) => panic!("{target} is a project, not {}", host.root.display()),
            }
        }
    }

    /// A path is taken as written, never split at a dot. §FS-rhei-library.2.2
    #[test]
    fn a_path_is_not_split_at_a_dot() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "v1.2/index.rhei.md", "# Rhei: Versioned\n");
        match resolve_into_target_from(dir.path(), "./v1.2").unwrap() {
            IntoTarget::Rhei(host) => {
                assert_eq!(host.root, dir.path().join("v1.2"));
                assert_eq!(host.parent, None);
            }
            IntoTarget::Project(_) => panic!("`./v1.2` is a rhei"),
        }
    }

    /// A path whose last segment is `<rhei>.<task>` for a rhei that exists
    /// there is refused with the bare spelling that does split, and from where;
    /// one naming no rhei keeps the plain refusal. §FS-rhei-library.2.2
    #[test]
    fn a_dotted_path_naming_a_rhei_gives_the_spelling_that_splits() {
        let dir = tempfile::tempdir().unwrap();
        let project = project_in(dir.path(), "panta");
        write(&project, "reports/index.rhei.md", "# Rhei: Reports\n");

        let help = |cwd: &Path, target: &str| {
            let Err(err) = resolve_into_target_from(cwd, target) else {
                panic!("`{target}` names a directory that does not exist");
            };
            err.help().map(|help| help.to_string()).unwrap_or_default()
        };
        let from_outside = help(dir.path(), "panta/reports.ticket");
        assert!(from_outside.contains("never split at a dot"), "{from_outside}");
        assert!(from_outside.contains("`--into reports.ticket`"), "{from_outside}");
        assert!(from_outside.contains("from 'panta'"), "{from_outside}");
        assert!(from_outside.contains("anywhere inside its project"), "{from_outside}");
        let from_inside = help(&project, "./reports.ticket");
        assert!(from_inside.contains("from this directory"), "{from_inside}");
        let nothing = help(dir.path(), "panta/nothing.ticket");
        assert!(!nothing.contains("never split at a dot"), "{nothing}");
    }

    /// A path to a single-file rhei's plan names no task of it: the hint is
    /// the rhei's bare id, never a task called `rhei.md`. §FS-rhei-library.2.2
    #[test]
    fn a_path_to_a_single_file_rheis_plan_gives_its_bare_id() {
        let dir = tempfile::tempdir().unwrap();
        let project = project_in(dir.path(), "proj");
        write(&project, "reports.rhei.md", "# Rhei: Reports\n");

        let Err(err) = resolve_into_target_from(dir.path(), "proj/reports.rhei.md") else {
            panic!("a plan file is not a directory to place into");
        };
        let help = err.help().map(|help| help.to_string()).unwrap_or_default();
        assert!(help.contains("`--into reports`"), "{help}");
        assert!(help.contains("from 'proj', or from anywhere inside its project"), "{help}");
        assert!(!help.contains("task"), "{help}");
    }

    /// The basin, a project with a task half, and a name that is both a rhei
    /// and a project are each refused, the last naming both. §FS-rhei-library.2.2
    #[test]
    fn the_basin_a_project_task_and_an_ambiguous_name_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let project = project_in(dir.path(), "reports");
        fs::create_dir_all(project.join("basin")).unwrap();
        for target in ["basin", "./basin"] {
            let refused = resolve_into_target_from(&project, target).err().expect(target).to_string();
            assert!(refused.contains("basin is never"), "{target}; got: {refused}");
        }

        let refused = resolve_into_target_from(dir.path(), "reports.3").err().unwrap().to_string();
        assert!(refused.contains("task '3'"), "got: {refused}");

        write(dir.path(), "reports.rhei.md", "# Rhei: Reports\n");
        let refused = resolve_into_target_from(dir.path(), "reports").err().unwrap().to_string();
        let rhei = display_slash(&dir.path().join("reports.rhei.md"));
        let project = display_slash(&project);
        assert!(refused.contains(&rhei) && refused.contains(&project), "names both; got: {refused}");
    }

    // ---- 3. the replacement check over a built set -------------------------

    /// A project under `housemachine`, whose working state is `work`, with:
    /// a single-file member and a basin ticket that run under the default, a
    /// member with a machine of its own, and a silent member whose ticket was
    /// already in a state no machine here defines.
    fn rebind_fixture(root: &Path) {
        write(root, "index.panta.md", "# Panta: Reports\n\n## Overview\n\nA project.\n");
        write(root, "states.yaml", &machine("housemachine", "work", &[]));
        write(root, "stranded.rhei.md", "# Rhei: Stranded\n\n## Tasks\n\n### Task one: A ticket\n**State:** work\n");
        write(root, "basin/001-capture.md", &ticket("work"));
        write(root, "own/index.rhei.md", "# Rhei: Own\n");
        write(root, "own/states.yaml", &machine("ownmachine", "work", &[]));
        write(root, "own/tasks/001-ticket.md", &ticket("work"));
        write(root, "broken/index.rhei.md", "# Rhei: Broken\n");
        write(root, "broken/tasks/001-ticket.md", &ticket("bogus"));
    }

    /// Build the machine set as the project stands, replace the root file, and
    /// build it again: what the second introduces is the rebind's.
    fn replace_default(root: &Path, replacement: &str) -> (Stranding, CheckedTickets) {
        let before = load_plan_for_validation(root).unwrap();
        let before_set = resolve_state_machines_for_loaded_plan(root, &before, None).unwrap();
        write(root, "states.yaml", replacement);
        let after = load_plan_for_validation(root).unwrap();
        let after_set = resolve_state_machines_for_loaded_plan(root, &after, None).unwrap();
        let introduced = errors_the_replacement_introduces(
            &after.rhei,
            Some(&before_set.validator_set()),
            &after_set.validator_set(),
        );
        (sort_introduced(&after.rhei, introduced), checked_tickets(&after, &after_set))
    }

    /// The silent member's and the basin's tickets are named with the state
    /// they hold; the member with a machine of its own is never read; and the
    /// ticket that was already invalid is the project's defect, not the
    /// rebind's, though the allowed list its error ends with changed.
    /// §FS-rhei-library.2.3
    #[test]
    fn a_replacement_strands_only_what_runs_under_the_default() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("reports");
        rebind_fixture(&root);

        let (stranding, checked) = replace_default(&root, &machine("laidmachine", "shipped", &[]));
        let named: Vec<(&str, &str)> =
            stranding.tickets.iter().map(|t| (t.id.as_str(), t.state.as_str())).collect();
        assert_eq!(named, [("stranded.one", "work"), ("basin.1", "work")], "{named:?}");
        assert!(stranding.kinds.is_empty(), "{:?}", stranding.kinds);
        // `stranded` and `broken` run under the default and `own` does not.
        assert_eq!((checked.tickets, checked.rheis, checked.basin), (3, 2, true));
    }

    /// A kind the replacement's `node_policy` routes that no structure
    /// declares is the rebind's, once. §FS-rhei-library.2.3 §FS-rhei-states.9.3
    #[test]
    fn a_kind_no_structure_declares_is_named_once() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("reports");
        rebind_fixture(&root);
        let (stranding, _) = replace_default(&root, &machine("laidmachine", "work", &["epic"]));
        assert!(stranding.tickets.is_empty(), "the working state is kept");
        assert_eq!(stranding.kinds, ["epic"]);
    }

    /// The two state errors compare by ticket and state alone; any other
    /// error compares whole, and each inherited one is spent once.
    /// §FS-rhei-library.2.3
    #[test]
    fn a_state_error_compares_without_the_allowed_list() {
        let inherited = vec![
            "Task a.1 has invalid state 'bogus'. Allowed: [work, completed]".to_owned(),
            "Task a.2 has state 'work' which is not allowed by its resolved profile. \
             Profile allows: [work]"
                .to_owned(),
            "something else".to_owned(),
        ];
        let after = vec![
            "Task a.1 has invalid state 'bogus'. Allowed: [shipped, completed]".to_owned(),
            "Task a.2 has state 'work' which is not allowed by its resolved profile. \
             Profile allows: [shipped]"
                .to_owned(),
            "Task a.3 has invalid state 'work'. Allowed: [shipped]".to_owned(),
            "something else".to_owned(),
            "something else".to_owned(),
        ];
        assert_eq!(
            errors_a_rebind_introduces(&inherited, after),
            ["Task a.3 has invalid state 'work'. Allowed: [shipped]", "something else"]
        );
    }
}
