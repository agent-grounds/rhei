// The union's own four decisions, each tested where it is made rather than
// through a placement that would exercise all of them at once.
// §AR-rhei-library.6.1 §FS-rhei-library.11 §FS-rhei-library.12
mod templates_union_tests {
    use super::super::*;

    const HOST: &str = r#"name: host
version: 1
states:
  pending:
    description: Ready for work
    instructions: |
      Do the work.
  completed:
    final: true
    description: Done
  cancelled:
    final: true
    description: Abandoned
transitions:
  - from: pending
    to: completed
  - from: "*"
    to: cancelled
profiles:
  host:
    initial: pending
    allowed: [pending, completed, cancelled]
node_policy:
  root: host
  default: host
  by_type:
    task: host
"#;

    /// Build a `PartMachine` from YAML text, the way a rendered template's own
    /// `states.yaml` arrives.
    fn part(name: &str, yaml: &str) -> PartMachine {
        PartMachine {
            name: name.to_owned(),
            text: yaml.to_owned(),
            value: serde_yaml::from_str(yaml).expect("the fixture parses"),
            machine: rhei_core::state_machine::StateMachine::parse_fragment(yaml)
                .expect("the fixture is a readable machine"),
        }
    }

    // ---- 1. rule-1 equality ------------------------------------------------

    /// `description` is not an operative field and declaration order is not a
    /// difference, so a state written two ways is one state.
    /// §FS-rhei-library.11.1
    #[test]
    fn rule_one_ignores_description_and_declaration_order() {
        // Same `pending`, fields reordered and the prose rewritten.
        let template = HOST
            .replace("name: host", "name: other")
            .replace(
                "  pending:\n    description: Ready for work\n    instructions: |\n      Do the work.\n",
                "  pending:\n    instructions: |\n      Do the work.\n    description: Quite different prose\n",
            );
        let unioned = union_machine(HOST, &part("other", &template))
            .expect("a reordered, reworded definition is the same definition");
        assert_eq!(
            unioned.matches("  pending:\n").count(),
            1,
            "one definition stands for both; got:\n{unioned}"
        );
    }

    /// An operative field that differs is refused naming both sources and the
    /// field, with no winner picked. §FS-rhei-library.11.1
    #[test]
    fn rule_one_refuses_a_differing_operative_field_naming_it() {
        let template = HOST
            .replace("name: host", "name: other")
            .replace("      Do the work.\n", "      Do something else.\n");
        let error = union_machine(HOST, &part("other", &template))
            .expect_err("a differing `instructions` is a refusal")
            .to_string();
        assert!(error.contains("pending"), "the refusal names the state: {error}");
        assert!(error.contains("instructions"), "the refusal names the field: {error}");
        assert!(error.contains("other"), "the refusal names the template: {error}");
    }

    // ---- 2. the terminal clause -------------------------------------------

    /// Two terminals coalesce by role rather than by spelling, so a template
    /// whose terminals are named something else — as the shipped
    /// `spec-implementation-discrepancy-audit` names its six, none of them
    /// `completed` — is not refused for having named them.
    /// §FS-rhei-library.11.1 §FS-rhei-states.1.4
    #[test]
    fn the_terminal_clause_is_a_role_test_not_a_name_test() {
        let host = HOST
            .replace("  completed:\n    final: true\n    description: Done\n", "  no-change:\n    final: true\n    description: Nothing to change\n")
            .replace("to: completed", "to: no-change")
            .replace("[pending, completed, cancelled]", "[pending, no-change, cancelled]");
        // The same two terminals, described differently by their other author.
        let template = host
            .replace("name: host", "name: audit")
            .replace("description: Nothing to change", "description: The audit found nothing")
            .replace("description: Abandoned", "description: Called off");
        let unioned = union_machine(&host, &part("audit", &template))
            .expect("two terminals with the same role coalesce whatever they are called");
        assert_eq!(
            unioned.matches("  no-change:\n").count(),
            1,
            "the terminal is added once; got:\n{unioned}"
        );
        assert_eq!(unioned.matches("  cancelled:\n").count(), 1, "and so is the cancellation");
    }

    // ---- 3. `sources:` derivation ------------------------------------------

    /// A template's `from: "*"` edge is written with `sources:` set to that
    /// template's own non-terminal states, and the host's own wildcard is left
    /// exactly as written. §FS-rhei-library.11.2
    #[test]
    fn a_templates_wildcard_is_scoped_to_its_own_states() {
        let template = r#"name: review-loop
version: 1
states:
  review:
    description: Read it
    instructions: |
      Read it.
  decide:
    description: Decide
    instructions: |
      Decide.
  completed:
    final: true
    description: Done
  cancelled:
    final: true
    description: Abandoned
transitions:
  - from: review
    to: decide
  - from: decide
    to: completed
  - from: "*"
    to: cancelled
profiles:
  review-loop:
    initial: review
    allowed: [review, decide, completed, cancelled]
node_policy:
  root: review-loop
  default: review-loop
  by_type:
    step: review-loop
"#;
        let unioned =
            union_machine(HOST, &part("review-loop", template)).expect("the union holds");
        let machine = rhei_core::state_machine::StateMachine::from_yaml_str(&unioned)
            .expect("the unioned machine is valid");
        let scoped: Vec<&Vec<String>> =
            machine.transitions.iter().filter_map(|rule| rule.sources.as_ref()).collect();
        assert_eq!(scoped.len(), 1, "exactly the template's wildcard is scoped: {scoped:?}");
        assert_eq!(
            scoped[0],
            &vec!["review".to_owned(), "decide".to_owned()],
            "the source set is the template's own non-terminal states"
        );
        // The host's wildcard spans the union, which is the host's call.
        assert!(
            machine
                .transitions
                .iter()
                .any(|rule| rule.from.0 == "*" && rule.sources.is_none()),
            "the host's own wildcard stays unscoped"
        );
        assert!(
            machine.transition_matches_source(
                machine
                    .transitions
                    .iter()
                    .find(|rule| rule.from.0 == "*" && rule.sources.is_none())
                    .expect("the host's wildcard"),
                "review",
            ),
            "so it reaches the states the host took in"
        );
    }

    // ---- 4. node_policy projection ----------------------------------------

    /// At union only a template's `by_type` entries survive: its `root`,
    /// `default` and `rhei` are dropped because the host's stand, and its
    /// `overrides` because they key on a level that is a fact about the
    /// placement. §FS-rhei-library.11.3
    #[test]
    fn only_a_templates_by_type_routes_survive_the_union() {
        let template = HOST
            .replace("name: host", "name: other")
            .replace("  host:\n    initial: pending", "  other:\n    initial: pending")
            .replace(
                "node_policy:\n  root: host\n  default: host\n  by_type:\n    task: host\n",
                "node_policy:\n  root: other\n  default: other\n  rhei: other\n  by_type:\n    step: other\n  overrides:\n    - match:\n        level: 2\n      profile: other\n",
            );
        let unioned = union_machine(HOST, &part("other", &template)).expect("the union holds");
        let policy: YamlValue = serde_yaml::from_str(&unioned).expect("the result parses");
        let policy = policy.get("node_policy").expect("the host keeps its policy");
        assert_eq!(
            policy.get("root").and_then(YamlValue::as_str),
            Some("host"),
            "the host's root stands"
        );
        assert_eq!(
            policy.get("default").and_then(YamlValue::as_str),
            Some("host"),
            "and so does its default"
        );
        assert!(policy.get("rhei").is_none(), "the template's `rhei` is dropped");
        assert!(
            policy.get("overrides").is_none(),
            "and its `overrides`, which key on where the host put it"
        );
        let by_type = policy.get("by_type").expect("by_type survives");
        assert_eq!(by_type.get("task").and_then(YamlValue::as_str), Some("host"));
        assert_eq!(
            by_type.get("step").and_then(YamlValue::as_str),
            Some("other"),
            "the template's own route joins the host's"
        );
    }

    // ---- 9. byte-exact serialization ---------------------------------------

    /// The union inserts the bytes the author wrote, which is why `--dry-run`
    /// can promise a diff of added lines and nothing else — a profile that
    /// declares `transition_limit` renders it exactly, and one that omits it
    /// still omits it. §FS-rhei-library.15.1
    #[test]
    fn a_declared_transition_limit_is_inserted_byte_for_byte() {
        let template = HOST
            .replace("name: host", "name: other")
            .replace(
                "profiles:\n  host:\n    initial: pending\n    allowed: [pending, completed, cancelled]\n",
                "profiles:\n  other:\n    initial: pending\n    transition_limit: 3\n    allowed: [pending, completed, cancelled]\n",
            )
            .replace("    task: host\n", "    step: other\n");
        let unioned = union_machine(HOST, &part("other", &template)).expect("the union holds");
        assert!(
            unioned.contains("    transition_limit: 3\n"),
            "the field is written as its author wrote it; got:\n{unioned}"
        );
        let machine = rhei_core::state_machine::StateMachine::from_yaml_str(&unioned)
            .expect("the unioned machine is valid");
        let profiles = machine.profiles.expect("profiles survive");
        assert_eq!(profiles["other"].transition_limit, Some(3));
        assert_eq!(
            profiles["host"].transition_limit, None,
            "a profile that declares no bound still declares none"
        );
    }

    // ---- 5. input union ----------------------------------------------------

    /// A template directory holding `template.yaml`, a states file, an index
    /// and one ticket: the smallest thing that is a template.
    fn write_template(dir: &Path, name: &str, manifest_tail: &str) -> PathBuf {
        let template = dir.join(name);
        std::fs::create_dir_all(template.join("tasks")).expect("create template");
        std::fs::write(
            template.join("template.yaml"),
            format!("name: {name}\nversion: 1.0.0\ndescription: A template\n{manifest_tail}"),
        )
        .expect("write manifest");
        std::fs::write(template.join("states.yaml"), HOST).expect("write states");
        std::fs::write(
            template.join("index.rhei.md"),
            "# Rhei: A template\n**States:** host\n\n---\nstructure:\n  maxLevels: 2\n  nodeKinds:\n  - task\n---\n",
        )
        .expect("write index");
        std::fs::write(
            template.join("tasks/001-own.md"),
            "### Task own: Its own ticket\n**State:** pending\n",
        )
        .expect("write ticket");
        template
    }

    fn union_test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rhei-union-unit-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create the test directory");
        dir
    }

    /// Two templates that declare an input of the same name declare one input,
    /// and an input only an included template declares joins the union — so an
    /// including template is used exactly as a flat one is.
    /// §FS-rhei-library.11.4
    #[test]
    fn inputs_union_across_an_inclusion() {
        let dir = union_test_dir("inputs");
        let host = write_template(
            &dir,
            "outer",
            "inputs:\n  - name: change_ref\n    description: The change\n    type: string\nincludes:\n  - inner\n",
        );
        write_template(
            &host,
            "inner",
            "inputs:\n  - name: change_ref\n    description: The change\n    type: string\n  - name: reviewers\n    description: How many\n    type: number\n    default: 2\n",
        );
        let manifest = load_template_manifest(&host).expect("the manifest loads");
        let inputs = union_inputs(&host, &manifest).expect("the inputs union");
        let names: Vec<&str> = inputs.iter().map(|input| input.name.as_str()).collect();
        assert_eq!(names, vec!["change_ref", "reviewers"], "one `change_ref`, plus the included one");
    }

    /// An including template fixes an included value by declaring the input
    /// itself, and its declaration wins over any included default.
    /// §FS-rhei-library.11.4
    #[test]
    fn an_including_templates_declaration_wins_over_an_included_default() {
        let dir = union_test_dir("inputs-wins");
        let host = write_template(
            &dir,
            "outer",
            "inputs:\n  - name: reviewers\n    description: Fixed by the host\n    type: number\n    default: 1\nincludes:\n  - inner\n",
        );
        write_template(
            &host,
            "inner",
            "inputs:\n  - name: reviewers\n    description: How many\n    type: number\n    default: 2\n",
        );
        let manifest = load_template_manifest(&host).expect("the manifest loads");
        let inputs = union_inputs(&host, &manifest).expect("the inputs union");
        assert_eq!(inputs.len(), 1, "one input, not two: {inputs:?}");
        assert_eq!(inputs[0].description, "Fixed by the host");
    }

    /// Two included defaults that differ with no declaration above them are an
    /// error naming both templates and the input. §FS-rhei-library.11.4
    #[test]
    fn two_differing_included_defaults_are_refused_naming_both() {
        let dir = union_test_dir("inputs-clash");
        let host = write_template(&dir, "outer", "inputs: []\nincludes:\n  - one\n  - two\n");
        write_template(
            &host,
            "one",
            "inputs:\n  - name: reviewers\n    description: How many\n    type: number\n    default: 2\n",
        );
        write_template(
            &host,
            "two",
            "inputs:\n  - name: reviewers\n    description: How many\n    type: number\n    default: 3\n",
        );
        let manifest = load_template_manifest(&host).expect("the manifest loads");
        let error = union_inputs(&host, &manifest)
            .expect_err("two differing defaults with nothing above them is a refusal")
            .to_string();
        assert!(error.contains("one"), "the refusal names the first template: {error}");
        assert!(error.contains("two"), "and the second: {error}");
        assert!(error.contains("reviewers"), "and the input: {error}");
    }

    // ---- 6. re-parenting, applied twice ------------------------------------

    /// One rendered task file, as `read_part_tickets` produces it.
    fn ticket_file(slug: &str, body: &str) -> PartTickets {
        PartTickets {
            slug: slug.to_owned(),
            body: body.to_owned(),
            metadata: BTreeMap::new(),
            ids: declared_ids(body),
        }
    }

    const REVIEW_FILE: &str = "### Step coordinate: Coordinate\n**State:** review\n\nGo.\n\n### Step record: Record\n**State:** review\n**Prior:** coordinate\n**Consumes:** coordinate:findings\n\nWrite it.\n";

    /// Re-parenting rewrites the heading id, deepens the heading, and rewrites
    /// every `**Prior:**` and `**Consumes:**` that names a template task.
    /// §FS-rhei-library.12
    #[test]
    fn reparenting_rewrites_ids_headings_and_references() {
        let mut files = vec![ticket_file("review", REVIEW_FILE)];
        reparent(&mut files, Some("ticket"));
        let body = &files[0].body;
        assert!(body.contains("#### Step ticket.coordinate: Coordinate"), "got:\n{body}");
        assert!(body.contains("#### Step ticket.record: Record"), "got:\n{body}");
        assert!(body.contains("**Prior:** ticket.coordinate"), "got:\n{body}");
        assert!(
            body.contains("**Consumes:** ticket.coordinate:findings"),
            "the export name is kept and only the task id moves; got:\n{body}"
        );
        assert_eq!(files[0].ids, vec!["ticket.coordinate", "ticket.record"]);
    }

    /// `under:` inside a template and `--into <rhei>.<task>` outside it are the
    /// same function applied twice, and they compose: this is what makes "a
    /// template works at any level" a fact about one code path.
    /// §FS-rhei-library.12.1 §AR-rhei-library.6.2
    #[test]
    fn two_reparentings_compose() {
        let mut files = vec![ticket_file("review", REVIEW_FILE)];
        reparent(&mut files, Some("ticket"));
        reparent(&mut files, Some("release"));
        let body = &files[0].body;
        assert!(
            body.contains("##### Step release.ticket.coordinate: Coordinate"),
            "the heading deepens once per application; got:\n{body}"
        );
        assert!(body.contains("**Prior:** release.ticket.coordinate"), "got:\n{body}");
        assert_eq!(files[0].ids, vec!["release.ticket.coordinate", "release.ticket.record"]);
    }

    /// A reference that names no template task is left exactly as written, so a
    /// cross-rhei `**Prior:**` survives a placement. §FS-rhei-library.12
    #[test]
    fn a_reference_outside_the_template_is_left_alone() {
        let body = "### Step record: Record\n**State:** review\n**Prior:** other-rhei.gate\n";
        let mut files = vec![ticket_file("record", body)];
        reparent(&mut files, Some("ticket"));
        assert!(
            files[0].body.contains("**Prior:** other-rhei.gate"),
            "got:\n{}",
            files[0].body
        );
    }

    // ---- 7. `under:` resolution against the accumulated tree ---------------

    /// `under:` resolves against the task tree as it stands when the entry is
    /// reached, so an id an earlier entry placed is available to a later one,
    /// and one nothing has placed is an error listing what is.
    /// §FS-rhei-library.14.1
    #[test]
    fn under_resolves_against_the_tree_accumulated_so_far() {
        let base = vec![(PathBuf::from("tasks/001-ticket.md"), vec!["ticket".to_owned()])];
        check_parent_exists("ticket", &base).expect("the host's own task resolves");
        let error = check_parent_exists("nowhere", &base)
            .expect_err("a name nothing has placed is an error")
            .to_string();
        assert!(error.contains("nowhere"), "the refusal names the entry's `under:`: {error}");

        // An earlier entry's placement joins the tree the next one resolves in.
        let mut files = vec![ticket_file("review", REVIEW_FILE)];
        reparent(&mut files, Some("ticket"));
        let mut grown = base.clone();
        grown[0].1.extend(files[0].ids.clone());
        check_parent_exists("ticket.coordinate", &grown)
            .expect("an id an earlier entry placed is available to a later one");
    }

    /// The same template under two parents is legal — different parents,
    /// different ids — and twice under one is the id collision.
    /// §FS-rhei-library.14.1 §FS-rhei-library.12
    #[test]
    fn the_same_template_under_two_parents_is_two_id_sets() {
        let mut first = vec![ticket_file("review", REVIEW_FILE)];
        reparent(&mut first, Some("ticket"));
        let mut second = vec![ticket_file("review", REVIEW_FILE)];
        reparent(&mut second, Some("second"));
        let host = vec![(
            PathBuf::from("tasks/001-ticket.md"),
            vec!["ticket".to_owned(), "second".to_owned()],
        )];
        check_id_collisions(&first[0].ids, &host).expect("the first parent's ids are free");
        check_id_collisions(&second[0].ids, &host).expect("and so are the second's");
        assert_ne!(first[0].ids, second[0].ids, "two parents, two id sets");

        // Twice under one parent is the collision, with the placement message.
        let after_first = vec![(PathBuf::from("tasks/001-ticket.md"), first[0].ids.clone())];
        let error = check_id_collisions(&first[0].ids, &after_first)
            .expect_err("twice under one parent is refused")
            .to_string();
        assert_eq!(
            error,
            "cannot place ticket 'ticket.coordinate': task id already exists in target"
        );
    }

    // ---- 8. the identity strip ---------------------------------------------

    /// A rendered entry that declares a `budgetTicketId` is refused, whether it
    /// is the index's `metadata.tasks` or a task file's own block — and the key
    /// is removed unconditionally from every cloned entry.
    /// §FS-rhei-library.13
    #[test]
    fn budget_identities_are_refused_at_the_source_and_stripped_from_the_clone() {
        let identity = |uuid: &str| -> YamlValue {
            serde_yaml::from_str(&format!("budgetTicketId: {uuid}\nvisits: 3\n"))
                .expect("the fixture parses")
        };
        let uuid = "11111111-2222-3333-4444-555555555555";

        // In the index's own `metadata.tasks`.
        let index: BTreeMap<String, YamlValue> =
            [("coordinate".to_owned(), identity(uuid))].into_iter().collect();
        let error = refuse_declared_identity("review-loop", &index, &[])
            .expect_err("an index-declared identity is refused")
            .to_string();
        assert!(error.contains("budgetTicketId"), "{error}");
        assert!(error.contains("coordinate"), "{error}");

        // And in a task file's own metadata block, which §FS-rhei-library.12
        // re-keys the same way — the hole the refusal would have if it read
        // only the index.
        let mut file = ticket_file("coordinate", "### Step coordinate: Go\n**State:** review\n");
        file.metadata = [("coordinate".to_owned(), identity(uuid))].into_iter().collect();
        let error = refuse_declared_identity("review-loop", &BTreeMap::new(), &[file])
            .expect_err("a task file's own block is scanned too")
            .to_string();
        assert!(error.contains("budgetTicketId"), "{error}");

        // The strip is unconditional, and takes nothing else with it.
        let mut cloned: BTreeMap<String, YamlValue> =
            [("ticket.coordinate".to_owned(), identity(uuid))].into_iter().collect();
        strip_identity(&mut cloned);
        let entry = &cloned["ticket.coordinate"];
        assert!(entry.get("budgetTicketId").is_none(), "the key is gone: {entry:?}");
        assert_eq!(
            entry.get("visits").and_then(YamlValue::as_u64),
            Some(3),
            "and the rest of the entry travels: {entry:?}"
        );
    }
}
