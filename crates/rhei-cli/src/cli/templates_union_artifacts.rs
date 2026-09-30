    // Artifact paths across a union: the refusal when two states claim one
    // rhei-scoped path, and the warning when two placed tickets walk one.
    //
    // Both halves read the same declarations; they differ in who is doing the
    // claiming, which is why one refuses and the other does not. Two states
    // sharing a path is a contradiction in the machine. Two *tickets* walking
    // one state's path is a choice the author is allowed to make — one
    // supervisor writing one plan note per rhei is correct — so it is said out
    // loud and left alone.

    // §FS-rhei-library.7.2

    /// Two states declaring one rhei-scoped artifact path — no per-task
    /// variable in it — is a union-time collision. §FS-rhei-library.7.2
    fn check_artifact_paths(host_text: &str, part: &PartMachine) -> MietteResult<()> {
        let host: YamlValue = serde_yaml::from_str(host_text).unwrap_or(YamlValue::Null);
        let mut claimed: BTreeMap<String, String> = BTreeMap::new();
        for (source, states) in [(&host, "the target"), (&part.value, part.name.as_str())] {
            let _ = states;
            let Some(YamlValue::Mapping(states)) = source.get("states") else {
                continue;
            };
            for (name, def) in states {
                let Some(name) = name.as_str() else { continue };
                for key in ["inputs", "outputs"] {
                    let Some(YamlValue::Sequence(items)) = def.get(key) else { continue };
                    for item in items {
                        let Some(path) = item.get("path").and_then(YamlValue::as_str) else {
                            continue;
                        };
                        if path.contains("{task_id}") {
                            continue;
                        }
                        match claimed.get(path) {
                            Some(other) if other != name => {
                                return Err(miette!(
                                    help = "put `{task_id}` in the path, or have one of the two \
                                            states declare a path of its own.",
                                    "states '{other}' and '{name}' both declare the rhei-scoped \
                                     artifact path '{path}'"
                                ));
                            }
                            Some(_) => {}
                            None => {
                                claimed.insert(path.to_owned(), name.to_owned());
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }


    /// One state's rhei-scoped path walked by two placed tickets: a warning
    /// naming the path and every ticket, and never a refusal.
    /// §FS-rhei-library.7.2
    fn warn_shared_artifact_paths(part: &PartMachine, tickets: &[PartTickets]) {
        let walkers = placed_ticket_states(part, tickets);
        for (state, paths) in rhei_scoped_paths(&part.value) {
            let mut named: Vec<&str> = walkers
                .iter()
                .filter(|(_, states)| states.contains(&state))
                .map(|(id, _)| id.as_str())
                .collect();
            named.sort_unstable();
            if named.len() < 2 {
                continue;
            }
            for path in paths {
                eprintln!(
                    "warning: state '{state}' declares '{path}', which is scoped to the rhei \
                     rather than to a task, and {} placed tickets walk it: {}. The second to \
                     reach that state overwrites the first — put `{{task_id}}` in the path if \
                     that is not what you want.",
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

    /// Every state of a machine and the declared paths it claims for the whole
    /// rhei — the ones carrying no per-task variable. §FS-rhei-library.7.2
    fn rhei_scoped_paths(machine: &YamlValue) -> Vec<(String, Vec<String>)> {
        let Some(YamlValue::Mapping(states)) = machine.get("states") else {
            return Vec::new();
        };
        states
            .iter()
            .filter_map(|(name, def)| {
                let name = name.as_str()?;
                let paths: Vec<String> = ["inputs", "outputs"]
                    .into_iter()
                    .filter_map(|key| def.get(key)?.as_sequence())
                    .flatten()
                    .filter_map(|item| item.get("path")?.as_str())
                    .filter(|path| !path.contains("{task_id}"))
                    .map(str::to_owned)
                    .collect();
                (!paths.is_empty()).then(|| (name.to_owned(), paths))
            })
            .collect()
    }
