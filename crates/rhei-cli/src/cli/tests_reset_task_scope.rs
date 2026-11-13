// The three rules a `--task` reset rests on that its end-to-end tests reach only
// through whole runs: the closure and the edge each ticket is listed by, the
// stripping that keeps a kept ticket's lines in a file it shares, and the
// ticket a file matched by prefix or component pattern belongs to.

// §AR-source-file-size.3 §FS-rhei-reset.2.5

mod reset_task_scope {
    use super::super::*;

    const MACHINE: &str = "name: scope
version: 1
states:
  pending: { initial: true, description: Open }
  supervising: { description: Supervise, execute_on: child-terminal, agent: pi }
  completed: { final: true, description: Done }
transitions:
  - { from: pending, to: completed, description: Done }
  - { from: supervising, to: completed, description: Done }
  - { from: supervising, to: supervising, description: Released }
";

    /// `x.1` finished over two children, `x.2` after it with a child of its own,
    /// `x.3` apart, and `x.4` a supervisor over a chain.
    const PLAN: &str = "# Rhei: X

---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: parent
**State:** completed

#### Task 1.1: first child
**State:** completed

#### Task 1.2: second child
**State:** completed

### Task 2: after the parent
**State:** pending
**Prior:** Task 1

#### Task 2.1: its child
**State:** pending

### Task 3: apart
**State:** pending

### Task 4: supervisor
**State:** supervising

#### Task 4.1: implement
**State:** completed

#### Task 4.2: review
**State:** completed
**Prior:** Task 4.1
";

    fn machines() -> rhei_validator::MachineSet {
        let machine = rhei_validator::StateMachine::from_yaml_str(MACHINE).expect("machine");
        rhei_validator::MachineSet { default: machine, per_rhei: BTreeMap::new() }
    }

    fn plan(dir: &tempfile::TempDir, text: &str) -> (PathBuf, LoadedPlan) {
        let path = dir.path().join("x.rhei.md");
        fs::write(&path, text).expect("write the plan");
        let loaded = load_plan(&path).expect("load the plan");
        (path, loaded)
    }

    fn closure(loaded: &LoadedPlan, named: &[&str]) -> TicketScope {
        let named: Vec<String> = named.iter().map(|id| id.to_string()).collect();
        TicketScope::close(loaded, &named, None, &machines())
    }

    fn edges(scope: &TicketScope) -> Vec<(&str, Option<String>)> {
        scope.reached.iter().map(|(id, edge)| (id.as_str(), edge.label())).collect()
    }

    /// A finished parent is taken alone and listed by the ticket under it, while its
    /// dependent comes with its child; the sibling and the unrelated ticket stay.
    /// §FS-rhei-reset.2.5.1 §FS-rhei-reset.2.5.2
    #[test]
    fn the_closure_takes_a_finished_parent_alone_and_lists_each_ticket_by_its_edge() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (_, loaded) = plan(&dir, PLAN);
        let scope = closure(&loaded, &["x.1.2"]);
        assert_eq!(
            edges(&scope),
            vec![
                ("x.1", Some("(finished parent of x.1.2)".to_string())),
                ("x.1.2", None),
                ("x.2", Some("(prior: x.1)".to_string())),
                ("x.2.1", Some("(child of x.2)".to_string())),
            ]
        );
    }

    /// A named ticket brings its descendants and dependents, never its priors.
    /// §FS-rhei-reset.2.5.1
    #[test]
    fn a_named_tickets_priors_stay_out_of_the_closure() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (_, loaded) = plan(&dir, PLAN);
        let scope = closure(&loaded, &["x.2"]);
        assert_eq!(edges(&scope), vec![("x.2", None), ("x.2.1", Some("(child of x.2)".to_string()))]);
        assert!(scope.supervisors.is_empty());
    }

    /// A ticket first reached as a finished ancestor and then by a dependent edge brings
    /// its descendants after all, and keeps the edge that reached it first.
    /// §FS-rhei-reset.2.5.1
    #[test]
    fn a_finished_ancestor_reached_again_as_a_dependent_brings_its_children() {
        let dir = tempfile::tempdir().expect("tempdir");
        let text = PLAN.replacen("### Task 1: parent\n**State:** completed\n", "### Task 1: parent\n**State:** completed\n**Prior:** Task 3\n", 1);
        let (_, loaded) = plan(&dir, &text);
        let scope = closure(&loaded, &["x.1.2", "x.3"]);
        assert_eq!(scope.edge("x.1"), Some(&ScopeEdge::FinishedParentOf("x.1.2".to_string())));
        assert_eq!(scope.edge("x.1.1"), Some(&ScopeEdge::ChildOf("x.1".to_string())));
    }

    /// The supervisor a scope leaves out is recorded with the tickets in scope it
    /// owns, in plan order; one the scope takes is not. §FS-rhei-reset.2.5.3
    #[test]
    fn a_supervisor_the_scope_leaves_out_is_recorded_with_the_tickets_it_owns() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (_, loaded) = plan(&dir, PLAN);
        let scope = closure(&loaded, &["x.4.1"]);
        assert_eq!(
            scope.supervisors,
            BTreeMap::from([("x.4".to_string(), vec!["x.4.1".to_string(), "x.4.2".to_string()])])
        );
        assert!(closure(&loaded, &["x.4"]).supervisors.is_empty());
    }

    /// A ticket scope strips the claims and result links under its own tickets'
    /// headings only; a reset of whole rheis strips every one. §FS-rhei-reset.2.5
    /// §FS-rhei-reset.2.4
    #[test]
    fn a_ticket_scope_strips_runtime_lines_under_its_own_headings_only() {
        let raw = "# Rhei: X\n\n## Tasks\n\n### Task 1: kept\n**State:** completed\n**Assignee:** a\n\n\
                   > **Result:** [runtime/results/x.1.md](runtime/results/x.1.md)\n\n\
                   ### Task 2: reset\n**State:** completed\n**Assignee:** b\n\n\
                   > **Result:** [runtime/results/x.2.md](runtime/results/x.2.md)\n";
        let authored = BTreeMap::from([("2".to_string(), "pending".to_string())]);
        let tickets = TicketScope {
            reached: BTreeMap::from([("x.2".to_string(), ScopeEdge::Named)]),
            resolved_in: None,
            supervisors: BTreeMap::new(),
        };
        let scope = ResetScope::Tickets(&tickets);
        let text = reset_file_text(raw, &authored, PlanFileKind::Plan, reset_strips_under(scope, &authored))
            .expect("rewrite");
        assert!(text.contains("**Assignee:** a\n\n> **Result:** [runtime/results/x.1.md]"), "{text}");
        assert!(text.contains("### Task 2: reset\n**State:** pending\n"), "{text}");
        assert!(!text.contains("**Assignee:** b") && !text.contains("x.2.md"), "{text}");

        let whole: RheiScope = None;
        let all = reset_file_text(raw, &authored, PlanFileKind::Plan, reset_strips_under((&whole).into(), &authored))
            .expect("rewrite");
        assert!(!all.contains("**Assignee:**") && !all.contains("> **Result:**"), "{all}");
        assert_eq!(strip_assignee_lines(raw, PlanFileKind::Plan), strip_assignee_lines_under(raw, PlanFileKind::Plan, |_| true));
    }

    /// A file a shorter id's prefix matches belongs to the longest id whose prefix
    /// matches it while that ticket is kept, so `x.build` leaves `x.build-docs`'s.
    /// §FS-rhei-reset.2.1
    #[test]
    fn a_prefix_matched_file_goes_to_the_longest_id_whose_prefix_it_matches() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (_, loaded) = plan(
            &dir,
            "# Rhei: X\n\n## Tasks\n\n### Task build: build\n**State:** pending\n\n\
             ### Task build-docs: docs\n**State:** pending\n",
        );
        let logs = dir.path().join("runtime/logs");
        fs::create_dir_all(&logs).expect("logs");
        for name in ["task-x.build-pending.log", "task-x.build-docs-pending.log"] {
            fs::write(logs.join(name), "log\n").expect("a log");
        }

        let scope = closure(&loaded, &["x.build"]);
        let owners = RuntimeOwners::index(&loaded, ResetScope::Tickets(&scope));
        let log = |id: &str| ScopedTarget::Prefixed(NamedPrefix::new(logs.clone(), "task-", id));
        let short = owners.entries(&log("x.build")).expect("list");
        assert_eq!(short, vec![logs.join("task-x.build-pending.log")]);
        let long = owners.entries(&log("x.build-docs")).expect("list");
        assert_eq!(long, vec![logs.join("task-x.build-docs-pending.log")]);
    }

    /// A placeholder before `{task_id}` matches one path component and never a `/`,
    /// and a path a shorter id's pattern matches is the longest id's whose pattern
    /// matches it too. §FS-rhei-reset.2.1
    #[test]
    fn a_component_pattern_never_crosses_a_slash_and_goes_to_the_longest_id() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        for file in [
            "runtime/review/x.build-1.md",
            "runtime/review/x.build-docs-1.md",
            "runtime/review/nested/x.build-1.md",
            "runtime/results/x.build.md",
        ] {
            fs::create_dir_all(root.join(file).parent().expect("a parent")).expect("dirs");
            fs::write(root.join(file), "x\n").expect("a file");
        }
        let template = "runtime/{state}/{task_id}-{visit_count}.md";
        assert!(ComponentPattern::applies_to(template));
        assert!(ComponentPattern::applies_to("runtime/out/{task_id}-{state}.md"));
        assert!(!ComponentPattern::applies_to("runtime/out/{task_id}.md"));

        let short = ComponentPattern::new(root, template, "x.build");
        let long = ComponentPattern::new(root, template, "x.build-docs");
        let mut found = short.entries().expect("list");
        found.sort();
        let (mine, theirs) =
            (root.join("runtime/review/x.build-1.md"), root.join("runtime/review/x.build-docs-1.md"));
        assert_eq!(found, vec![mine.clone(), theirs.clone()]);

        let owners = RuntimeOwners { kept: ids(&["x.build", "x.build-docs"]), ..Default::default() };
        assert_eq!(owned(&owners, &short), vec![mine.clone()]);
        assert_eq!(owned(&owners, &long), vec![theirs.clone()]);

        // `{state}` reaches results/ and supervise/, and `{visit_count}` takes `docs`:
        // x.build's pattern matches x.build-docs's result and brief, which stay theirs.
        for file in ["runtime/results/x.build-docs.md", "runtime/supervise/x.build-docs.md"] {
            fs::create_dir_all(root.join(file).parent().expect("a parent")).expect("dirs");
            fs::write(root.join(file), "theirs\n").expect("a file");
            assert!(short.entries().expect("list").contains(&root.join(file)), "the wildcards reach {file}");
        }
        assert_eq!(owned(&owners, &short), vec![mine]);
        assert_eq!(owned(&owners, &long), vec![theirs]);
    }

    fn ids(ids: &[&str]) -> BTreeSet<String> {
        ids.iter().map(|id| id.to_string()).collect()
    }

    /// The paths `pattern` matches that its ticket owns against `owners`.
    fn owned(owners: &RuntimeOwners, pattern: &ComponentPattern) -> Vec<PathBuf> {
        let mut paths = owners.entries(&ScopedTarget::Pattern(pattern.clone())).expect("list");
        paths.sort();
        paths
    }

    /// Under every shape of declared path with another placeholder, before the id or
    /// after it, a ticket's pattern owns only that ticket's own files, though a longer
    /// id's output, result and brief lie where its placeholders reach. The last column
    /// is more of the longer id's: its brief beside an id-first path, which `{state}`
    /// taking `docs` matches, and its first output in a directory of two. §FS-rhei-reset.2.1
    #[test]
    fn a_component_pattern_owns_only_its_own_tickets_files_under_every_shape() {
        let shapes: [(&str, &str, &str, &[&str]); 8] = [
            ("runtime/{state}/{task_id}.md", "runtime/pending/x.build.md", "runtime/pending/x.build-docs.md", &[]),
            ("runtime/{state}/{task_id}-{visit_count}.md", "runtime/pending/x.build-1.md", "runtime/pending/x.build-docs-1.md", &[]),
            ("runtime/{state}/{task_id}{visit_count}.md", "runtime/pending/x.build1.md", "runtime/pending/x.build-docs1.md", &[]),
            ("runtime/{state}-{task_id}-{visit_count}.md", "runtime/pending-x.build-1.md", "runtime/pending-x.build-docs-1.md", &[]),
            ("runtime/{state}/{task_id}/out.md", "runtime/pending/x.build/out.md", "runtime/pending/x.build-docs/out.md", &[]),
            ("runtime/supervise/{task_id}-{state}.md", "runtime/supervise/x.build-pending.md", "runtime/supervise/x.build-docs-pending.md", &[]),
            ("runtime/fix-summary/{task_id}.{visit_count}.md", "runtime/fix-summary/x.build.1.md", "runtime/fix-summary/x.build-docs.1.md", &["runtime/fix-summary/x.build-docs.md"]),
            ("runtime/notes/{task_id}-{state}.md", "runtime/notes/x.build-pending.md", "runtime/notes/x.build-docs-pending.md", &["runtime/notes/x.build-docs.md"]),
        ];
        let owners = RuntimeOwners { kept: ids(&["x.build", "x.build-docs", "build", "build-docs"]), ..Default::default() };
        for (template, mine, theirs, also) in shapes {
            let dir = tempfile::tempdir().expect("tempdir");
            let root = dir.path();
            let own = vec![mine, "runtime/results/x.build.md", "runtime/supervise/x.build.md"];
            let mut other = vec![theirs, "runtime/results/x.build-docs.md", "runtime/supervise/x.build-docs.md"];
            other.extend(also);
            for file in own.iter().chain(&other) {
                fs::create_dir_all(root.join(file).parent().expect("a parent")).expect("dirs");
                fs::write(root.join(file), "x\n").expect("a file");
            }
            for (id, expected, files) in [("x.build", mine, &own), ("x.build-docs", theirs, &other)] {
                let found = owned(&owners, &ComponentPattern::new(root, template, id));
                assert!(found.contains(&root.join(expected)), "{template}: {id} owns {expected}: {found:?}");
                assert!(
                    found.iter().all(|path| files.iter().any(|file| root.join(file) == *path)),
                    "{template}: {id} owns only its own files: {found:?}"
                );
            }
        }
    }

    /// Only a ticket the reset keeps claims a path a longer id of its could stand in. One
    /// it reaches stakes no claim, so the path is taken, and that ticket is counted as
    /// losing it too; the same for a name rhei writes. §FS-rhei-reset.2.1 §FS-rhei-reset.2.5.3
    #[test]
    fn only_a_kept_ticket_claims_a_path_its_longer_id_could_stand_in() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let summaries = root.join("runtime/fix-summary");
        let logs = root.join("runtime/logs");
        fs::create_dir_all(&summaries).expect("summaries");
        fs::create_dir_all(&logs).expect("logs");
        for name in ["x.1.1.md", "x.1.2.md", "x.1.1.1.md"] {
            fs::write(summaries.join(name), "x\n").expect("a summary");
        }
        for name in ["task-x.build-pending.log", "task-x.build-docs-pending.log"] {
            fs::write(logs.join(name), "log\n").expect("a log");
        }
        let reached = |ids: &[&str]| -> BTreeMap<String, BTreeSet<String>> {
            ids.iter().map(|id| (id.to_string(), BTreeSet::from([id.to_string()]))).collect()
        };
        let sorted = |owners: &RuntimeOwners, target: &ScopedTarget| {
            let mut paths = owners.entries(target).expect("list");
            paths.sort();
            paths
        };
        let standing = |owners: &RuntimeOwners, target: &ScopedTarget, path: &Path| {
            owners.standing(target, path).cloned().collect::<Vec<_>>()
        };

        // The ci-heal summary: `x.1.1` could stand in `x.1.1.md`, `x.1`'s first summary.
        let summary = ScopedTarget::Pattern(ComponentPattern::new(
            root,
            "runtime/fix-summary/{task_id}.{visit_count}.md",
            "x.1",
        ));
        let kept = RuntimeOwners { kept: ids(&["x.1.1"]), reached: reached(&["x.1"]) };
        assert_eq!(sorted(&kept, &summary), vec![summaries.join("x.1.2.md")]);
        let both = RuntimeOwners { kept: BTreeSet::new(), reached: reached(&["x.1", "x.1.1"]) };
        let all = ["x.1.1.1.md", "x.1.1.md", "x.1.2.md"].map(|name| summaries.join(name));
        assert_eq!(sorted(&both, &summary), all.to_vec());
        assert_eq!(standing(&both, &summary, &all[0]), vec!["x.1.1"]);
        assert_eq!(standing(&both, &summary, &all[1]), vec!["x.1.1"]);
        assert!(standing(&both, &summary, &all[2]).is_empty(), "no longer id stands in x.1.2.md");

        // A log rhei writes: `x.build-docs`'s is `x.build`'s only while it is reached too.
        let log = ScopedTarget::Prefixed(NamedPrefix::new(logs.clone(), "task-", "x.build"));
        let (mine, theirs) = (logs.join("task-x.build-pending.log"), logs.join("task-x.build-docs-pending.log"));
        let kept = RuntimeOwners { kept: ids(&["x.build-docs"]), reached: reached(&["x.build"]) };
        assert_eq!(sorted(&kept, &log), vec![mine.clone()]);
        let both = RuntimeOwners { kept: BTreeSet::new(), reached: reached(&["x.build", "x.build-docs"]) };
        assert_eq!(sorted(&both, &log), vec![theirs.clone(), mine]);
        assert_eq!(standing(&both, &log, &theirs), vec!["x.build-docs"]);
    }

    /// Below its first placeholder a pattern enters no symbolic link, whether at a
    /// placeholder level or a literal one; a link it matches as its last component
    /// is removed as the link, never what it points at. §FS-rhei-reset.2.1
    #[cfg(unix)]
    #[test]
    fn a_component_pattern_never_descends_through_a_symbolic_link() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().expect("tempdir");
        let (root, outside) = (dir.path().join("root"), dir.path().join("outside"));
        for file in ["root/runtime/pending/out/x.1.md", "outside/out/x.1.md", "outside/dir/keep.md", "outside/file.md"] {
            fs::create_dir_all(dir.path().join(file).parent().expect("a parent")).expect("dirs");
            fs::write(dir.path().join(file), "x\n").expect("a file");
        }
        for level in ["review", "done/out", "kept/out"] {
            fs::create_dir_all(root.join("runtime").join(level)).expect("dirs");
        }
        symlink(&outside, root.join("runtime/elsewhere")).expect("a link at a placeholder level");
        symlink(outside.join("out"), root.join("runtime/review/out")).expect("a link at a literal level");
        symlink(outside.join("dir"), root.join("runtime/done/out/x.1.md")).expect("a last link to a dir");
        symlink(outside.join("file.md"), root.join("runtime/kept/out/x.1.md")).expect("a last link to a file");

        let pattern = ComponentPattern::new(&root, "runtime/{state}/out/{task_id}.md", "x.1");
        let mut found = pattern.entries().expect("list");
        found.sort();
        let links = [root.join("runtime/done/out/x.1.md"), root.join("runtime/kept/out/x.1.md")];
        assert_eq!(found, vec![links[0].clone(), links[1].clone(), root.join("runtime/pending/out/x.1.md")]);

        let mut removed = Vec::new();
        for link in &links {
            assert!(remove_runtime_path(link, &mut removed).expect("remove"), "{link:?} goes");
            assert!(fs::symlink_metadata(link).is_err(), "the link itself is gone");
        }
        for file in ["out/x.1.md", "dir/keep.md", "file.md"] {
            assert_eq!(fs::read_to_string(outside.join(file)).expect("kept"), "x\n", "{file} is never reached");
        }
    }

    /// The kept line names five tickets and counts the rest. §FS-rhei-reset.4
    #[test]
    fn the_kept_line_names_five_tickets_and_counts_the_rest() {
        let kept: Vec<String> = (2..=8).map(|n| format!("z.{n}")).collect();
        assert_eq!(
            kept_tickets_line(&kept).as_deref(),
            Some("Kept 7 task(s) as they stand, with their results and logs: z.2, z.3, z.4, z.5, z.6 and 2 more")
        );
        assert_eq!(kept_tickets_line(&kept[..1]).as_deref(), Some("Kept 1 task(s) as they stand, with their results and logs: z.2"));
        assert_eq!(kept_tickets_line(&[]), None);
    }
}
