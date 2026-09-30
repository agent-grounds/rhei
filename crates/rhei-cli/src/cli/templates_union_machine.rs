    // The graph union itself: whether two authored definitions are the same
    // thing, scoping a template's wildcard to its own states, projecting its
    // node policy, and inserting what is new at the end of each block.

    // §FS-rhei-library.11 §AR-rhei-library.6.1

    /// One template's machine, as both text and parsed value, ready to join a
    /// host's. §FS-rhei-library.11
    struct PartMachine {
        /// The template's name, which every refusal names as a source.
        name: String,
        text: String,
        value: YamlValue,
        machine: rhei_core::state_machine::StateMachine,
    }

    impl PartMachine {
        /// Load a rendered template's machine.
        ///
        /// Parsed rather than validated: a union's own refusals are about two
        /// authors disagreeing, and they read better than the same machine's
        /// standalone defect reported from inside a placement. The whole result
        /// is validated before anything is written either way.
        /// §FS-rhei-library.11.1 §AR-rhei-library.6.4
        fn load(name: &str, path: &Path) -> MietteResult<Self> {
            let text = fs::read_to_string(path)
                .map_err(|err| file_io_report(path, "failed to read the template's states", err))?;
            let value: YamlValue = serde_yaml::from_str(&text).map_err(|err| {
                miette!(
                    help = "a template's states.yaml must parse on its own before it can join a machine.",
                    "failed to parse '{}': {err}",
                    path.display()
                )
            })?;
            let machine = rhei_core::state_machine::StateMachine::parse_fragment(&text)
                .map_err(|err| {
                    miette!(
                        help = "every template stands alone, so its states.yaml is readable                                 before it joins a machine.",
                        "template '{name}' has an unreadable state machine: {err}"
                    )
                })?;
            Ok(Self { name: name.to_owned(), text, value, machine })
        }

        /// The template's own non-terminal states, which is the `sources:` set
        /// its `from: "*"` edge is written with. §FS-rhei-library.11.2
        fn wildcard_sources(&self) -> Vec<String> {
            self.machine
                .states
                .iter()
                .filter(|(_, def)| !def.terminal)
                .map(|(name, _)| name.clone())
                .collect()
        }
    }

    /// Union `part`'s machine into `host_text`, returning the host's bytes with
    /// the part's entries inserted at the end of each block.
    ///
    /// Nothing about the host is rewritten: every result is the host's text
    /// with lines added, which is what lets `--dry-run` promise a diff of added
    /// lines and nothing else. §FS-rhei-library.10 §FS-rhei-library.15.1
    fn union_machine(host_text: &str, part: &PartMachine) -> MietteResult<String> {
        let host_value: YamlValue = serde_yaml::from_str(host_text).map_err(|err| {
            miette!(help = union_target_help(), "the target's states.yaml does not parse: {err}")
        })?;
        let host_blocks = yaml_blocks(host_text);
        let part_blocks = yaml_blocks(&part.text);
        let mut insertions: Vec<(usize, String)> = Vec::new();
        let mut appended = String::new();

        for key in ["states", "profiles"] {
            let added = union_named_block(part, &host_value, &host_blocks, &part_blocks, key)?;
            push_block_addition(&mut insertions, &mut appended, &host_blocks, key, added);
        }
        let added = union_transitions(part, &host_value, &host_blocks, &part_blocks)?;
        push_block_addition(&mut insertions, &mut appended, &host_blocks, "transitions", added);
        let added = union_models(&host_value, &part.value, &host_blocks);
        push_block_addition(&mut insertions, &mut appended, &host_blocks, "models", added);

        let mut out = host_text.to_owned();
        insertions.sort_by(|left, right| right.0.cmp(&left.0));
        for (at, text) in insertions {
            out.insert_str(at, &text);
        }
        if !appended.is_empty() {
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&appended);
        }
        union_node_policy(&mut out, part, &host_value)?;
        Ok(out)
    }

    /// Record one block's addition: into the block when the host has it, at the
    /// end of the file as a whole block when it does not.
    fn push_block_addition(
        insertions: &mut Vec<(usize, String)>,
        appended: &mut String,
        host_blocks: &IndexMap<String, YamlBlock>,
        key: &str,
        added: String,
    ) {
        if added.is_empty() {
            return;
        }
        match host_blocks.get(key) {
            Some(block) => insertions.push((block.body_end, added)),
            None => {
                appended.push_str(key);
                appended.push_str(":\n");
                appended.push_str(&added);
            }
        }
    }

    /// `states` and `profiles`: a named entry the host already defines must be
    /// the same thing, and one it does not is inserted as its author wrote it.
    /// §FS-rhei-library.11.1
    fn union_named_block(
        part: &PartMachine,
        host_value: &YamlValue,
        host_blocks: &IndexMap<String, YamlBlock>,
        part_blocks: &IndexMap<String, YamlBlock>,
        key: &str,
    ) -> MietteResult<String> {
        let Some(part_block) = part_blocks.get(key) else {
            return Ok(String::new());
        };
        let host_indent = host_blocks.get(key).map_or(2, |block| block.indent);
        let mut added = String::new();
        for entry in &part_block.entries {
            let part_entry = part.value.get(key).and_then(|node| node.get(&entry.key));
            let host_entry = host_value.get(key).and_then(|node| node.get(&entry.key));
            if let (Some(host_entry), Some(part_entry)) = (host_entry, part_entry) {
                if same_definition(key, &entry.key, host_entry, part_entry, part) {
                    continue;
                }
                return Err(union_conflict(part, key, &entry.key, host_entry, part_entry));
            }
            added.push_str(&reindent(&part.text, &entry.range, part_block.indent, host_indent));
        }
        Ok(added)
    }

    /// Whether two same-named definitions are the same thing.
    ///
    /// For a terminal the test is its role rather than its spelling, so a
    /// template whose terminals are named something else is not refused for
    /// having named them. §FS-rhei-library.11.1
    fn same_definition(
        key: &str,
        name: &str,
        host_entry: &YamlValue,
        part_entry: &YamlValue,
        part: &PartMachine,
    ) -> bool {
        if canonical(host_entry) == canonical(part_entry) {
            return true;
        }
        if key != "states" {
            return false;
        }
        let final_on_both = [host_entry, part_entry]
            .iter()
            .all(|entry| entry.get("final").and_then(YamlValue::as_bool) == Some(true));
        final_on_both && part.machine.is_cancellation(name) == host_is_cancellation(host_entry, name)
    }

    /// The host side of the terminal clause, read from the entry alone: a
    /// machine that does not load yet cannot answer it. §FS-rhei-states.1.4
    fn host_is_cancellation(entry: &YamlValue, name: &str) -> bool {
        rhei_core::state_machine::is_cancelled_state_name(name)
            || entry.get("role").and_then(YamlValue::as_str) == Some("cancellation")
    }

    /// The refusal rule 1 owes: both sources and the field they disagree on,
    /// and no winner picked. §FS-rhei-library.11.1
    fn union_conflict(
        part: &PartMachine,
        key: &str,
        name: &str,
        host_entry: &YamlValue,
        part_entry: &YamlValue,
    ) -> Report {
        let noun = key.trim_end_matches('s');
        let field = differing_field(host_entry, part_entry)
            .unwrap_or_else(|| "its definition".to_owned());
        miette!(
            help = format!(
                "a union renames nothing, so the two must agree: change one of them, or give \
                 template '{}' a prefix input so it can declare a {noun} of its own.",
                part.name
            ),
            "{noun} '{name}' is defined by both the target and template '{}', and they differ \
             on '{field}'",
            part.name
        )
    }

    /// `transitions`: a template's `from: "*"` edge is scoped to that
    /// template's own states on the way in, and an edge the host already has is
    /// added once. §FS-rhei-library.11.2
    fn union_transitions(
        part: &PartMachine,
        host_value: &YamlValue,
        host_blocks: &IndexMap<String, YamlBlock>,
        part_blocks: &IndexMap<String, YamlBlock>,
    ) -> MietteResult<String> {
        let Some(part_block) = part_blocks.get("transitions") else {
            return Ok(String::new());
        };
        let host_indent = host_blocks.get("transitions").map_or(2, |block| block.indent);
        let host_rules: Vec<&YamlValue> = host_value
            .get("transitions")
            .and_then(YamlValue::as_sequence)
            .map(|rules| rules.iter().collect())
            .unwrap_or_default();
        let sources = part.wildcard_sources();
        let mut added = String::new();
        for (index, entry) in part_block.entries.iter().enumerate() {
            let Some(part_rule) = part
                .value
                .get("transitions")
                .and_then(YamlValue::as_sequence)
                .and_then(|rules| rules.get(index))
            else {
                continue;
            };
            let (part_rule, text) = scope_wildcard(part, part_rule, entry, host_indent, &sources);
            let same_edge = |rule: &&&YamlValue| {
                rule.get("from") == part_rule.get("from")
                    && rule.get("to") == part_rule.get("to")
                    && rule.get("sources") == part_rule.get("sources")
            };
            if let Some(host_rule) = host_rules.iter().find(same_edge) {
                if canonical(host_rule) == canonical(&part_rule) {
                    continue;
                }
                let edge = format!(
                    "{} -> {}",
                    part_rule.get("from").and_then(YamlValue::as_str).unwrap_or("?"),
                    part_rule.get("to").and_then(YamlValue::as_str).unwrap_or("?")
                );
                return Err(union_conflict(part, "transitions", &edge, host_rule, &part_rule));
            }
            added.push_str(&text);
        }
        Ok(added)
    }

    /// A `from: "*"` rule rewritten with `sources:` set to the template's own
    /// states, as both value and text. An already-scoped rule is its author's
    /// and is left alone. §FS-rhei-library.11.2
    fn scope_wildcard(
        part: &PartMachine,
        rule: &YamlValue,
        entry: &YamlEntry,
        host_indent: usize,
        sources: &[String],
    ) -> (YamlValue, String) {
        let part_indent = yaml_blocks(&part.text).get("transitions").map_or(2, |b| b.indent);
        let text = reindent(&part.text, &entry.range, part_indent, host_indent);
        let wildcard = rule.get("from").and_then(YamlValue::as_str) == Some("*");
        if !wildcard || rule.get("sources").is_some() || sources.is_empty() {
            return (rule.clone(), text);
        }
        let mut scoped = rule.clone();
        if let YamlValue::Mapping(map) = &mut scoped {
            let list = sources.iter().map(|name| YamlValue::String(name.clone())).collect();
            map.insert(YamlValue::String("sources".into()), YamlValue::Sequence(list));
        }
        let field = " ".repeat(host_indent + 2);
        let item = " ".repeat(host_indent + 4);
        let mut block = format!("{field}sources:\n");
        for name in sources {
            block.push_str(&format!("{item}- {name}\n"));
        }
        let mut lines = text.splitn(2, '\n');
        let head = lines.next().unwrap_or_default();
        let tail = lines.next().unwrap_or_default();
        (scoped, format!("{head}\n{block}{tail}"))
    }

    /// `models` joins as a set: a name both sides declare is one name.
    /// §FS-rhei-library.10
    fn union_models(
        host_value: &YamlValue,
        part_value: &YamlValue,
        host_blocks: &IndexMap<String, YamlBlock>,
    ) -> String {
        let listed = |value: &YamlValue| -> Vec<String> {
            value
                .get("models")
                .and_then(YamlValue::as_sequence)
                .map(|items| {
                    items.iter().filter_map(|item| item.as_str().map(str::to_owned)).collect()
                })
                .unwrap_or_default()
        };
        let host = listed(host_value);
        let indent = host_blocks.get("models").map_or(2, |block| block.indent);
        let mut added = String::new();
        for model in listed(part_value).into_iter().filter(|model| !host.contains(model)) {
            added.push_str(&" ".repeat(indent));
            added.push_str("- ");
            added.push_str(&model);
            added.push('\n');
        }
        added
    }

    /// `node_policy`: only the template's `by_type` routes survive, because the
    /// host's `root`, `default` and `rhei` stand and its `overrides` key on a
    /// level that is a fact about the placement. §FS-rhei-library.11.3
    fn union_node_policy(
        out: &mut String,
        part: &PartMachine,
        host_value: &YamlValue,
    ) -> MietteResult<()> {
        let part_routes = part.value.get("node_policy").and_then(|policy| policy.get("by_type"));
        let Some(YamlValue::Mapping(part_routes)) = part_routes else {
            return Ok(());
        };
        let host_routes = host_value.get("node_policy").and_then(|policy| policy.get("by_type"));
        let mut added = String::new();
        for (kind, profile) in part_routes {
            let (Some(kind), Some(profile)) = (kind.as_str(), profile.as_str()) else {
                continue;
            };
            match host_routes.and_then(|routes| routes.get(kind)).and_then(YamlValue::as_str) {
                Some(existing) if existing == profile => continue,
                Some(existing) => {
                    return Err(miette!(
                        help = "a union renames nothing: route the template's tickets through a node kind of its own.",
                        "node kind '{kind}' is routed to '{existing}' by the target and to \
                         '{profile}' by template '{}'",
                        part.name
                    ));
                }
                None => added.push_str(&format!("    {kind}: {profile}\n")),
            }
        }
        if added.is_empty() {
            return Ok(());
        }
        insert_into_by_type(out, &added)
    }

    /// Insert routes at the end of `node_policy.by_type`, creating the map when
    /// the host routes nothing by kind yet. §FS-rhei-library.11.3
    fn insert_into_by_type(out: &mut String, added: &str) -> MietteResult<()> {
        let Some(policy) = yaml_blocks(out).get("node_policy").cloned() else {
            out.push_str("node_policy:\n  by_type:\n");
            out.push_str(added);
            return Ok(());
        };
        let Some(by_type) = policy.entries.iter().find(|entry| entry.key == "by_type") else {
            let at = policy.body_end;
            out.insert_str(at, &format!("  by_type:\n{added}"));
            return Ok(());
        };
        let end = out[by_type.range.clone()]
            .rfind(|c: char| !c.is_whitespace())
            .map_or(by_type.range.end, |offset| {
                by_type.range.start
                    + out[by_type.range.start + offset..]
                        .find('\n')
                        .map_or(offset + 1, |newline| offset + newline + 1)
            });
        out.insert_str(end, added);
        Ok(())
    }

    /// The whole of provenance: one comment line per inclusion, which the YAML
    /// parser discards and a reader does not. §FS-rhei-library.15.1
    fn fence_comment(
        name: &str,
        version: &str,
        digest: &str,
        inputs: &BTreeMap<String, serde_json::Value>,
    ) -> String {
        let rendered: Vec<String> =
            inputs.iter().map(|(key, value)| format!("{key}: {value}")).collect();
        format!("# --- {name} {version} src:sha256:{digest} inputs: {{{}}} ---\n", rendered.join(", "))
    }

    fn union_target_help() -> &'static str {
        "run `rhei validate <rhei>` on the target first: a union is written into a machine that \
         already loads."
    }
