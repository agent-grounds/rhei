// The hold a placement takes, its own part for the reason `new_lock.rs` is:
// serializing a write knows nothing about ids, markdown or state machines, only
// which file stands for the scope being written to. §FS-rhei-library.2 §FS-rhei-new.4
mod templates_union_hold_tests {
    use super::super::*;

    /// A member's placement writes the *project's* settings, so the sidecar
    /// that stands for the whole write is the project's, not the member's.
    /// Taking the member's would leave two unions into two members of one
    /// project racing over that one file. §FS-rhei-library.2 §FS-rhei-new.4
    #[test]
    fn a_member_placement_locks_the_project_and_a_standalone_one_its_own_root() {
        let tmp = tempfile::tempdir().expect("a temp dir");
        let project = tmp.path().join("panta");
        let member = project.join("host");
        std::fs::create_dir_all(&member).expect("the fixture tree");
        std::fs::write(project.join("index.panta.md"), "# Panta: fixture\n").expect("a manifest");
        std::fs::write(member.join("index.rhei.md"), "# Rhei: host\n").expect("an index");

        let host = UnionHost {
            index: member.join("index.rhei.md"),
            machine: member.join("states.yaml"),
            project: union_project(&member, false),
            root: member.clone(),
            single_file: false,
            parent: None,
        };
        assert_eq!(union_scope_root(&host), project, "the scope is the project");
        assert_eq!(
            new_create_lock_path(union_scope_root(&host)),
            project.join("index.panta.md"),
            "and so is the sidecar the placement is serialized on"
        );

        // Outside a project the rhei's own root is the scope, unchanged.
        let lone = tmp.path().join("lone");
        std::fs::create_dir_all(&lone).expect("the fixture tree");
        std::fs::write(lone.join("index.rhei.md"), "# Rhei: lone\n").expect("an index");
        let host = UnionHost {
            index: lone.join("index.rhei.md"),
            machine: lone.join("states.yaml"),
            project: union_project(&lone, false),
            root: lone.clone(),
            single_file: false,
            parent: None,
        };
        assert_eq!(union_scope_root(&host), lone, "no project, so the rhei stands for itself");
        assert_eq!(new_create_lock_path(union_scope_root(&host)), lone.join("index.rhei.md"));
    }
}
