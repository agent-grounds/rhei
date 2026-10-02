    // Artifact declarations are not filesystem effects: unions refuse cross-boundary
    // writers; hand-offs and internal pairs are silent. Shared files are advisories.
    // §FS-rhei-library.7.2.2 §FS-rhei-library.7.2.4 §AR-rhei-library.2

    /// Distinct declared writers and input-only readers of one rendered path.
    /// A both-lists state counts once, as a writer. §FS-rhei-library.7.2
    #[derive(Default)]
    struct ArtifactDeclarations {
        writers: BTreeSet<String>,
        readers: BTreeSet<String>,
    }

    /// Neither runtime task id spelling is rhei-scoped. §FS-rhei-library.7.2
    fn is_rhei_scoped_path(path: &str) -> bool {
        !path.contains("{task_id}") && !path.contains("{task_id_local}")
    }

    /// Classify equal rendered strings, without interpreting prose or scripts.
    /// Optional readers count; outputs take precedence per state and path.
    /// §FS-rhei-library.7.2 §FS-rhei-library.7.2.4
    fn classify_artifact_paths(machine: &YamlValue) -> BTreeMap<String, ArtifactDeclarations> {
        let mut paths: BTreeMap<String, ArtifactDeclarations> = BTreeMap::new();
        let Some(YamlValue::Mapping(states)) = machine.get("states") else {
            return paths;
        };
        for (name, def) in states {
            let Some(name) = name.as_str() else { continue };
            let declared = |key: &str| -> BTreeSet<&str> {
                def.get(key)
                    .and_then(YamlValue::as_sequence)
                    .into_iter()
                    .flatten()
                    .filter_map(|item| item.get("path").and_then(YamlValue::as_str))
                    .filter(|path| is_rhei_scoped_path(path))
                    .collect()
            };
            let outputs = declared("outputs");
            for path in &outputs {
                paths.entry((*path).to_owned()).or_default().writers.insert(name.to_owned());
            }
            for path in declared("inputs").difference(&outputs) {
                paths.entry((*path).to_owned()).or_default().readers.insert(name.to_owned());
            }
        }
        paths
    }

    /// The two authored sides of an artifact refusal. §FS-rhei-library.7.2.1
    #[derive(Clone, Copy)]
    enum ArtifactSides<'a> {
        Placement,
        Includes { including: &'a str, entry: &'a str },
    }

    /// Refuse only distinct writers across the boundary, never internal pairs
    /// or a hand-off. Same-named definitions are judged by ordinary rule 1.
    /// §FS-rhei-library.7.2.1 §FS-rhei-library.7.2.2 §FS-rhei-library.7.2.4 §FS-rhei-library.3.1
    fn check_artifact_paths(
        host_text: &str,
        part: &PartMachine,
        sides: ArtifactSides<'_>,
    ) -> MietteResult<()> {
        let host: YamlValue = serde_yaml::from_str(host_text).unwrap_or(YamlValue::Null);
        let host_paths = classify_artifact_paths(&host);
        let coalesces = |writer: &str| {
            match (
                host.get("states").and_then(|states| states.get(writer)),
                part.value.get("states").and_then(|states| states.get(writer)),
            ) {
                (Some(host_state), Some(part_state)) => {
                    same_definition("states", writer, host_state, part_state, part)
                }
                _ => false,
            }
        };
        let (host_side, part_side) = match sides {
            ArtifactSides::Placement => ("the target".to_owned(), format!("template '{}'", part.name)),
            ArtifactSides::Includes { including, entry } => {
                (format!("'{including}'"), format!("'{entry}'"))
            }
        };
        for (path, declarations) in classify_artifact_paths(&part.value) {
            let Some(host_declarations) = host_paths.get(&path) else {
                continue;
            };
            for other in &host_declarations.writers {
                for name in &declarations.writers {
                    if other == name {
                        continue;
                    }
                    // Ignore incoming outputs discarded with a coalesced definition. §FS-rhei-library.3.1
                    // Such a state is not a writer of this path in the result. §FS-rhei-library.7.2.1
                    if !host_declarations.writers.contains(name) && coalesces(name) {
                        continue;
                    }
                    // Internal pairs need a coalesced writer of this path on both sides. §FS-rhei-library.7.2.4
                    if [other, name].into_iter().any(|writer| {
                        host_declarations.writers.contains(writer)
                            && declarations.writers.contains(writer)
                            && coalesces(writer)
                    }) {
                        continue;
                    }
                    return Err(miette!(
                        help = "give the two states distinct artifact paths, or keep one of \
                                them as the path's writer and have the other list it under `inputs:`.",
                        "states '{other}' (in {host_side}) and '{name}' (in {part_side}) both \
                         declare the rhei-scoped artifact path '{path}' in `outputs:`"
                    ));
                }
            }
        }
        Ok(())
    }

    /// Shared input advisories computed on one completed machine, sorted by
    /// path and reader. Later included writers clear them. §FS-rhei-library.7.2.3
    fn shared_input_warnings(machine: &YamlValue) -> Vec<String> {
        classify_artifact_paths(machine)
            .into_iter()
            .filter(|(_, declarations)| {
                declarations.writers.is_empty() && declarations.readers.len() >= 2
            })
            .map(|(path, declarations)| {
                let readers = declarations.readers.into_iter().collect::<Vec<_>>().join(", ");
                format!(
                    "warning: shared input '{path}' has no declared output producer; readers: \
                     {readers}; it may be supplied by a program, callback, operator or instructions."
                )
            })
            .collect()
    }

    /// Read a machine actually laid or previewed. A member inheriting the
    /// project default brings no machine of its own to diagnose again.
    /// §FS-rhei-library.7.2.3 §AR-rhei-library.2
    fn shared_input_warnings_in_file(path: &Path) -> MietteResult<Vec<String>> {
        if !path.is_file() {
            return Ok(Vec::new());
        }
        let machine: YamlValue = serde_yaml::from_str(&read_text(path)?).map_err(|err| {
            miette!("failed to read the laid machine '{}': {err}", display_slash(path))
        })?;
        Ok(shared_input_warnings(&machine))
    }

    /// One state's rhei-scoped path walked by two placed tickets: a warning
    /// naming the path and every ticket, and never a refusal.
    /// §FS-rhei-library.7.2.5
    fn warn_shared_artifact_paths(part: &PartMachine, tickets: &[PartTickets]) {
        let walkers = placed_ticket_states(part, tickets);
        for (state, paths) in rhei_scoped_paths(&part.value) {
            let mut named: Vec<&str> = walkers
                .iter()
                .filter(|(_, states)| states.contains(&state))
                .map(|(id, _)| id.as_str())
                .collect();
            named.sort_unstable();
            named.dedup();
            if named.len() < 2 {
                continue;
            }
            for path in paths {
                eprintln!(
                    "warning: state '{state}' declares '{path}', which is scoped to the rhei \
                     rather than to a task, and {} placed tickets walk it: {}. These tickets \
                     share the same file; if work writes it, one ticket's content may overwrite \
                     another's. Keep the shared path when that is intentional, or use \
                     `{{task_id}}` for a per-ticket artifact — which works only where each ticket \
                     writes the file it reads.",
                    named.len(),
                    named.join(", ")
                );
            }
        }
    }

    /// Every state a placed ticket can reach, by placed id: its heading's kind
    /// routes it through the template's own `node_policy`, and the profile that
    /// answers says which states it is allowed. §FS-rhei-library.3.3
    fn placed_ticket_states(
        part: &PartMachine,
        tickets: &[PartTickets],
    ) -> Vec<(String, BTreeSet<String>)> {
        tickets
            .iter()
            .flat_map(|file| declared_kinds(&file.body))
            .filter_map(|(kind, id)| {
                let profile = routed_profile(&part.value, &kind)?;
                Some((id, profile_allowed(&part.value, &profile)))
            })
            .collect()
    }

    /// Every heading's kind and id, in document order.
    fn declared_kinds(body: &str) -> Vec<(String, String)> {
        let mut in_code_block = false;
        body.lines()
            .filter_map(|line| {
                let (hashes, id) = node_heading_outside_code(line, &mut in_code_block)?;
                let kind = line[hashes + 1..].split_whitespace().next()?;
                Some((kind.to_ascii_lowercase(), id.to_owned()))
            })
            .collect()
    }

    /// The profile a template routes a kind through: its own `by_type` entry,
    /// or the `default` it carries as a standalone rhei. §FS-rhei-states.9
    fn routed_profile(machine: &YamlValue, kind: &str) -> Option<String> {
        let policy = machine.get("node_policy")?;
        policy
            .get("by_type")
            .and_then(|by_type| by_type.get(kind))
            .or_else(|| policy.get("default"))
            .and_then(YamlValue::as_str)
            .map(str::to_owned)
    }

    fn profile_allowed(machine: &YamlValue, profile: &str) -> BTreeSet<String> {
        machine
            .get("profiles")
            .and_then(|profiles| profiles.get(profile))
            .and_then(|profile| profile.get("allowed"))
            .and_then(YamlValue::as_sequence)
            .map(|allowed| {
                allowed.iter().filter_map(|state| state.as_str().map(str::to_owned)).collect()
            })
            .unwrap_or_default()
    }

    /// Deduplicated rhei-scoped paths by state, from either list and using the
    /// classifier's shared per-task rule. §FS-rhei-library.7.2.5
    fn rhei_scoped_paths(machine: &YamlValue) -> BTreeMap<String, BTreeSet<String>> {
        let mut states: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (path, declarations) in classify_artifact_paths(machine) {
            for state in declarations.writers.into_iter().chain(declarations.readers) {
                states.entry(state).or_default().insert(path.clone());
            }
        }
        states
    }
