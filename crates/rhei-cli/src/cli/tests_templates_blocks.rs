mod templates_blocks_tests {
    use super::super::*;

    fn machine(text: &str) -> YamlValue {
        serde_yaml::from_str(text).expect("machine YAML")
    }

    /// Semantic fields are rewritten while descriptions and other free text
    /// remain untouched. §FS-rhei-library.4
    #[test]
    fn typed_reference_rewriting_qualifies_owned_machine_references() {
        let qualifier = Qualifier::new(vec!["review".into()]);
        let ownership = SettingsOwnership {
            agents: BTreeSet::from(["worker".into()]),
            models: BTreeSet::from(["smart".into()]),
            qualifier: qualifier.clone(),
            ..SettingsOwnership::default()
        };
        let qualified = qualify_machine(
            machine(
                r#"name: local
version: 1
models: [smart]
states:
  work:
    description: "work and smart stay prose"
    agent: worker
    model: smart
    prompt_template: shared
    outputs: [{name: report, path: runtime/report.md}]
  done: {final: true}
transitions: [{from: work, to: done}]
profiles: {primary: {initial: work, allowed: [work, done]}}
node_policy: {root: primary, default: primary}
"#,
            ),
            &qualifier,
            &ownership,
            Path::new("states.yaml"),
        )
        .expect("qualify machine");
        let text = serde_yaml::to_string(&qualified).expect("serialize");
        for expected in [
            "m6_review__work",
            "m6_review__done",
            "m6_review__worker",
            "m6_review__smart",
            "m6_review__shared",
            "runtime/blocks/m6_review__/runtime/report.md",
        ] {
            assert!(text.contains(expected), "missing {expected} in {text}");
        }
        assert!(text.contains("work and smart stay prose"));
    }

    fn leaf(alias: &str, entry: &str, done: &str) -> (CompiledLeaf, CompiledNode) {
        let qualifier = Qualifier::new(vec![alias.into()]);
        let entry = qualifier.qualify(entry);
        let done = qualifier.qualify(done);
        let machine = machine(&format!(
            "name: x\nversion: 1\nstates:\n  {entry}: {{}}\n  {done}: {{final: true}}\ntransitions: [{{from: {entry}, to: {done}}}]\nprofiles:\n  {}primary: {{initial: {entry}, allowed: [{entry}, {done}]}}\nnode_policy: {{root: {}primary, default: {}primary}}\n",
            qualifier.prefix(), qualifier.prefix(), qualifier.prefix()
        ));
        (
            CompiledLeaf {
                name: alias.into(),
                version: "1".into(),
                source: PathBuf::from(alias),
                qualifier,
                machine,
                tasks: Vec::new(),
                custom_kinds: BTreeSet::new(),
                settings: None,
                rendered: PathBuf::new(),
            },
            CompiledNode {
                name: alias.into(),
                manifest: PathBuf::from(alias).join("template.yaml"),
                entry: entry.clone(),
                exits: BTreeMap::from([("done".into(), done.clone())]),
                inputs: BTreeMap::new(),
                outputs: BTreeMap::new(),
                primary_states: vec![entry, done],
            },
        )
    }

    /// Only the exact seam source loses terminality, and the generated route
    /// follows the resolved mount order. §FS-rhei-library.5
    #[test]
    fn terminal_selection_and_outer_routing_follow_the_seam() {
        let (review_leaf, review) = leaf("review", "work", "done");
        let (fix_leaf, fix) = leaf("fix", "work", "done");
        let merged = merge_machines(
            "flow",
            &[review_leaf, fix_leaf],
            &[review.clone(), fix.clone()],
            &[ResolvedSeam { from: review.exits["done"].clone(), to: fix.entry.clone() }],
        )
        .expect("merge");
        let states = yaml_map(&merged, "states").expect("states");
        assert_eq!(
            states[YamlValue::String(review.exits["done"].clone())]["final"],
            YamlValue::Bool(false)
        );
        assert_eq!(
            states[YamlValue::String(fix.exits["done"].clone())]["final"],
            YamlValue::Bool(true)
        );
        assert_eq!(merged["profiles"]["flow"]["initial"], review.entry);
    }

    /// A file pass aliases the consumer input to the producer's already
    /// qualified path without changing requiredness. §FS-rhei-library.6
    #[test]
    fn state_file_pass_lowering_shares_the_producer_path() {
        let mut merged = machine(
            r#"name: flow
version: 1
states:
  produce:
    outputs: [{name: report, path: runtime/blocks/m1_a__/runtime/report.md}]
  consume:
    inputs: [{name: report, path: runtime/blocks/m1_b__/runtime/report.md, optional: true}]
  done: {final: true}
"#,
        );
        let endpoint = |state: &str, manifest: &str| ResolvedDataEndpoint {
            kind: DataKind::StateFile,
            state: Some(state.into()),
            task: None,
            name: "report".into(),
            manifest: PathBuf::from(manifest),
        };
        apply_state_file_passes(
            &mut merged,
            &[ResolvedPass {
                source: endpoint("produce", "a/template.yaml"),
                target: endpoint("consume", "b/template.yaml"),
                source_label: "a.report".into(),
                target_label: "b.report".into(),
            }],
        )
        .expect("lower pass");
        let consume = &merged["states"]["consume"]["inputs"][0];
        assert_eq!(consume["path"], "runtime/blocks/m1_a__/runtime/report.md");
        assert_eq!(consume["optional"], true);
    }

    /// Export passes add a qualified consumer reference and do not copy any
    /// runtime contents at instantiation. §FS-rhei-library.6
    #[test]
    fn task_export_pass_lowering_adds_consumes_metadata() {
        let mut leaves = vec![CompiledLeaf {
            name: "consumer".into(),
            version: "1".into(),
            source: PathBuf::new(),
            qualifier: Qualifier::default(),
            machine: YamlValue::Null,
            tasks: vec![(
                PathBuf::from("task.md"),
                "### Task target: Consume\n**State:** work\n\nDo work.\n".into(),
            )],
            custom_kinds: BTreeSet::new(),
            settings: None,
            rendered: PathBuf::new(),
        }];
        let endpoint = |task: &str, name: &str| ResolvedDataEndpoint {
            kind: DataKind::TaskExport,
            state: None,
            task: Some(task.into()),
            name: name.into(),
            manifest: PathBuf::new(),
        };
        apply_task_export_passes(
            &mut leaves,
            &[ResolvedPass {
                source: endpoint("source", "findings"),
                target: endpoint("target", "findings"),
                source_label: "a.findings".into(),
                target_label: "b.findings".into(),
            }],
        )
        .expect("lower export pass");
        assert!(leaves[0].tasks[0].1.contains("**Consumes:** source:findings"));
    }

    /// Nested mounts extend, rather than flatten, the alias chain. This pins
    /// recursive expansion's ownership identity. §FS-rhei-library.2 §FS-rhei-library.4
    #[test]
    fn recursive_expansion_preserves_every_alias_segment() {
        let root = Qualifier::new(vec!["outer".into()]);
        let first = root.child("review").qualify("done");
        let second = root.child("fix").qualify("done");
        assert_eq!(first, "m5_outer__m6_review__done");
        assert_eq!(second, "m5_outer__m3_fix__done");
        assert_ne!(first, second);
    }
}
