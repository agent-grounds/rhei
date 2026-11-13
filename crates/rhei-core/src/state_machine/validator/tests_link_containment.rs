    // A Directory Workspace link is held inside the root before its existence is
    // checked; a Single-File Plan's is not held at all. §FS-rhei-plan-language.3.6

    fn plan_linking(target: &str) -> Rhei {
        let input = format!(
            "# Rhei: Links\n\n## Tasks\n\n### Task 1: Load\n**State:** pending\n\n\
             Read [it]({target}).\n"
        );
        parse(&input).expect("parse ok")
    }

    /// A tree `<tmp>/ws/` beside `<tmp>/design/local-profile.json`.
    fn workspace_beside_outside_file(outside_file: bool) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tmpdir");
        std::fs::create_dir_all(dir.path().join("ws/docs")).expect("ws");
        std::fs::write(dir.path().join("ws/docs/x.md"), "# x\n").expect("x");
        if outside_file {
            std::fs::create_dir_all(dir.path().join("design")).expect("design");
            std::fs::write(dir.path().join("design/local-profile.json"), "{}\n").expect("file");
        }
        dir
    }

    /// §FS-rhei-plan-language.3.6: `.` is dropped and `..` folds into the segment
    /// before it, without reading the file system.
    #[test]
    fn normalize_lexically_folds_dot_segments() {
        assert_eq!(normalize_lexically(Path::new("/a/./b/../c")), PathBuf::from("/a/c"));
        assert_eq!(normalize_lexically(Path::new("/a/b/../../..")), PathBuf::from("/"));
        assert_eq!(normalize_lexically(Path::new("a/../../b")), PathBuf::from("../b"));
        assert_eq!(normalize_lexically(Path::new("./a/")), PathBuf::from("a"));
    }

    /// §FS-rhei-plan-language.3.6: containment is judged on the normalized path,
    /// so a link that leaves and returns stays inside.
    #[test]
    fn link_leaves_workspace_root_after_normalizing() {
        let root = Path::new("/p/ws");
        assert!(link_leaves_workspace_root(root, Path::new("../design/x.json")).is_some());
        assert!(link_leaves_workspace_root(root, Path::new("docs/../../x")).is_some());
        assert!(link_leaves_workspace_root(root, Path::new("../wsx/a")).is_some());
        assert!(link_leaves_workspace_root(root, Path::new("../ws/docs/x.md")).is_none());
        assert!(link_leaves_workspace_root(root, Path::new("./docs/x.md")).is_none());
    }

    /// §FS-rhei-plan-language.3.6: an escaping workspace link is refused as leaving
    /// the root whether or not a file sits where it lands.
    #[test]
    fn an_escaping_workspace_link_is_refused_whether_or_not_its_target_exists() {
        for outside_file in [true, false] {
            let dir = workspace_beside_outside_file(outside_file);
            let ws = dir.path().join("ws");
            let report = Validator::new(sample_machine()).validate_with_base(
                &plan_linking("../design/local-profile.json"),
                Some(LinkBase::workspace_root(&ws)),
            );
            assert_eq!(report.errors.len(), 1, "outside file {outside_file}: {:?}", report.errors);
            assert!(
                report.errors[0].contains("leaves the workspace root"),
                "outside file {outside_file}: {:?}",
                report.errors
            );
            assert!(report.errors[0].contains(&ws.display().to_string()), "{:?}", report.errors);
        }
    }

    /// §FS-rhei-plan-language.3.6: `../ws/docs/x.md` normalizes inside `ws` and is
    /// checked like any in-root link.
    #[test]
    fn a_workspace_link_that_leaves_and_returns_is_accepted() {
        let dir = workspace_beside_outside_file(false);
        let ws = dir.path().join("ws");
        let report = Validator::new(sample_machine())
            .validate_with_base(&plan_linking("../ws/docs/x.md"), Some(LinkBase::workspace_root(&ws)));
        assert!(!report.has_errors(), "{:?}", report.errors);

        let missing = Validator::new(sample_machine())
            .validate_with_base(&plan_linking("../ws/docs/y.md"), Some(LinkBase::workspace_root(&ws)));
        assert!(missing.errors.iter().any(|e| e.contains("does not exist")), "{:?}", missing.errors);
    }

    /// §FS-rhei-plan-language.3.6: containment is a Directory Workspace rule; a
    /// Single-File Plan's directory may be linked above.
    #[test]
    fn a_single_file_plan_link_above_its_directory_is_accepted() {
        let dir = workspace_beside_outside_file(true);
        let report = Validator::new(sample_machine()).validate_with_base(
            &plan_linking("../design/local-profile.json"),
            Some(LinkBase::plan_directory(&dir.path().join("ws"))),
        );
        assert!(!report.has_errors(), "{:?}", report.errors);
    }

    /// §AR-rhei-panta.5 §FS-rhei-plan-language.3.6: through per-task bases, a task
    /// is contained only when its owning root is a workspace root.
    #[test]
    fn per_task_bases_contain_only_workspace_roots() {
        let dir = workspace_beside_outside_file(true);
        let ws = dir.path().join("ws");
        let rhei = plan_linking("../design/local-profile.json");
        let tasks = HashMap::from([("1".to_string(), ws.clone())]);
        let check = |workspace_roots: &HashSet<PathBuf>| {
            let bases = LinkBases {
                default: LinkBase::plan_directory(dir.path()),
                tasks: &tasks,
                sections: &[],
                workspace_roots,
            };
            validate_with_machine_and_link_bases(&rhei, &sample_machine(), &bases)
        };

        let contained = check(&HashSet::from([ws.clone()]));
        assert!(contained.errors.iter().any(|e| e.contains("leaves the workspace root")), "{:?}", contained.errors);
        let single_file = check(&HashSet::new());
        assert!(!single_file.has_errors(), "{:?}", single_file.errors);
    }
