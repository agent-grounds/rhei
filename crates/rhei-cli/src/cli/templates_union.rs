    // `rhei instantiate <template> --into <rhei>[.<task>]`: the whole union,
    // held in memory until it validates and then written as one insertion per
    // block. §FS-rhei-library.1 §FS-rhei-library.2 §AR-rhei-library.5

    /// A rendered template waiting to join a host: its files, and what the
    /// fence comment records about where they came from.
    /// §FS-rhei-library.7.1
    struct RenderedPart {
        name: String,
        version: String,
        digest: String,
        inputs: BTreeMap<String, serde_json::Value>,
        /// The rendered workspace, which is an ordinary rhei.
        root: PathBuf,
    }

    /// Everything a placement would write, kept until the whole union
    /// validates. There is no staging directory, so a refusal leaves the target
    /// byte-identical without a rollback path to get wrong.
    /// §AR-rhei-library.5
    #[derive(Default)]
    struct UnionWrites {
        /// Whole new contents for a file the target owns.
        files: Vec<(PathBuf, String)>,
        /// Files copied beside the rhei's own, under rule 1.
        copies: Vec<(PathBuf, PathBuf)>,
        /// Lines the summary prints about what the union decided.
        notes: Vec<String>,
    }

    /// Place one rendered template into `host`, writing nothing until the
    /// result validates. §FS-rhei-library.1
    fn union_into_host(
        host: &UnionHost,
        part: &RenderedPart,
        declaration: &MachineDeclaration,
        mode: UnionMode,
    ) -> MietteResult<()> {
        let writes = plan_union(host, part, declaration)?;
        // An entry is validated with the rest: an including template's own
        // machine names states its parts bring, so it is a fragment until
        // every entry has joined it. §FS-rhei-library.6 §AR-rhei-library.5
        if mode != UnionMode::Compose {
            validate_union(host, &writes)?;
        }
        if mode == UnionMode::DryRun {
            print_union_diff(host, part, &writes);
            return Ok(());
        }
        apply_union(&writes)?;
        if mode == UnionMode::Compose {
            // An `includes:` entry is an implementation detail of the template
            // being instantiated; its summary is the instantiation's own.
            return Ok(());
        }
        println!(
            "Placed template '{}' into '{}'.",
            part.name,
            display_path(&host.index).display()
        );
        for note in &writes.notes {
            println!("  {note}");
        }
        Ok(())
    }

    /// Why a union is being written, which is what decides whether it reports
    /// for itself. §FS-rhei-library.2 §FS-rhei-library.6
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum UnionMode {
        /// `--into`: the union is the command, so it prints its own summary.
        Place,
        /// `--dry-run`: print the diff and write nothing.
        DryRun,
        /// An `includes:` entry, folded into the instantiation's own report.
        Compose,
    }

    /// The whole union, decided in memory. §AR-rhei-library.5
    fn plan_union(
        host: &UnionHost,
        part: &RenderedPart,
        declaration: &MachineDeclaration,
    ) -> MietteResult<UnionWrites> {
        let mut writes = UnionWrites::default();
        let machine_path = part.root.join("states.yaml");
        let part_machine = PartMachine::load(&part.name, &machine_path)?;

        let mut tickets = read_part_tickets(&part.root)?;
        let part_index = part.root.join("index.rhei.md");
        let part_plan = part.root.join("plan.rhei.md");
        let part_header = [part_index, part_plan]
            .into_iter()
            .find(|path| path.is_file())
            .map(|path| read_text(&path))
            .transpose()?
            .unwrap_or_default();
        let part_front = plan_frontmatter(&part_header)
            .as_deref()
            .map(parse_frontmatter)
            .transpose()?
            .unwrap_or(YamlValue::Null);
        let mut task_metadata = task_metadata_entries(&part_front).unwrap_or_default();
        refuse_declared_identity(&part.name, &task_metadata, &tickets)?;

        reparent(&mut tickets, host.parent.as_deref());
        if let Some(parent) = host.parent.as_deref() {
            let known: BTreeSet<String> = task_metadata.keys().cloned().collect();
            task_metadata = task_metadata
                .into_iter()
                .map(|(id, value)| {
                    let key = if known.contains(&id) { format!("{parent}.{id}") } else { id };
                    (key, value)
                })
                .collect();
        }
        for file in &tickets {
            task_metadata.extend(file.metadata.clone());
        }
        strip_identity(&mut task_metadata);

        let host_files = host_ticket_files(host)?;
        let placed: Vec<String> = tickets.iter().flat_map(|file| file.ids.clone()).collect();
        check_depth(&placed)?;
        check_id_collisions(&placed, &host_files)?;
        if let Some(parent) = host.parent.as_deref() {
            check_parent_exists(parent, &host_files)?;
        }

        let host_machine_text = read_text(&host.machine)?;
        check_artifact_paths(&host_machine_text, &part_machine)?;
        let mut machine = union_machine(&host_machine_text, &part_machine)?;
        if !machine.ends_with('\n') {
            machine.push('\n');
        }
        machine.push_str(&fence_comment(
            &part.name,
            &part.version,
            &part.digest,
            &part.inputs,
        ));
        writes.files.push((host.machine.clone(), machine));

        let kinds = declared_node_kinds(&part_front);
        let levels = placed.iter().map(|id| id.split('.').count() as u8).max().unwrap_or(1);
        let declared = match declaration {
            MachineDeclaration::Write(name) => {
                writes.notes.push(format!(
                    "added `**States:** {name}` to {}, without which the union would be inert",
                    display_path(&host.index).display()
                ));
                Some(name.as_str())
            }
            MachineDeclaration::Matches => None,
        };
        let index_raw = read_text(&host.index)?;
        let additions = IndexAdditions {
            kinds: &kinds,
            levels,
            tasks: &task_metadata,
            declaration: declared,
        };
        let mut index = union_index(&index_raw, &additions)?;
        if host.single_file {
            index = place_tickets_in_file(&index, host.parent.as_deref(), &tickets);
            writes.files.push((host.index.clone(), index));
        } else {
            writes.files.push((host.index.clone(), index));
            place_workspace_tickets(host, &host_files, &tickets, &mut writes)?;
        }
        copy_bundled_files(&part.root, &host.root, &mut writes)?;
        writes.notes.push(format!(
            "{} ticket(s) placed, {} state(s) brought",
            placed.len(),
            part_machine.machine.states.len()
        ));
        Ok(writes)
    }

    /// Ticket depth 4 is a hard ceiling — `######` is the deepest heading
    /// Markdown gives — and it wins over growing `maxLevels`.
    /// §FS-rhei-library.4.1
    fn check_depth(placed: &[String]) -> MietteResult<()> {
        let Some(overflow) = placed.iter().find(|id| id.split('.').count() > 4) else {
            return Ok(());
        };
        Err(miette!(
            help = "place the template higher in the tree: four is as deep as a ticket goes, \
                    because `######` is the deepest heading Markdown gives.",
            "cannot place ticket '{overflow}': it would sit {} levels deep, past the ceiling of 4",
            overflow.split('.').count()
        ))
    }

    /// A ticket id already taken in the target is refused before anything is
    /// written, so a template with tickets is placed once per parent.
    /// §FS-rhei-library.4
    fn check_id_collisions(
        placed: &[String],
        host_files: &[(PathBuf, Vec<String>)],
    ) -> MietteResult<()> {
        let taken: BTreeSet<&String> =
            host_files.iter().flat_map(|(_, ids)| ids.iter()).collect();
        let Some(clash) = placed.iter().find(|id| taken.contains(id)) else {
            return Ok(());
        };
        Err(miette!(
            help = "place it under another task, or author the second set of tickets in the \
                    states the first placement already brought.",
            "cannot place ticket '{clash}': task id already exists in target"
        ))
    }

    /// `--into <rhei>.<task>` names a task the target actually has.
    /// §FS-rhei-new.3
    fn check_parent_exists(
        parent: &str,
        host_files: &[(PathBuf, Vec<String>)],
    ) -> MietteResult<()> {
        let mut available: Vec<&String> =
            host_files.iter().flat_map(|(_, ids)| ids.iter()).collect();
        if available.iter().any(|id| *id == parent) {
            return Ok(());
        }
        available.sort();
        Err(miette!(
            help = format!(
                "the ids available in the target are: {}",
                available.iter().map(|id| id.as_str()).collect::<Vec<_>>().join(", ")
            ),
            "no task '{parent}' in the target to place under"
        ))
    }

    /// A budget identity belongs to a run and never to a template, so a
    /// rendered entry that declares one is refused before anything is written.
    /// §FS-rhei-library.5
    fn refuse_declared_identity(
        name: &str,
        index_tasks: &BTreeMap<String, YamlValue>,
        tickets: &[PartTickets],
    ) -> MietteResult<()> {
        let declared = index_tasks
            .iter()
            .chain(tickets.iter().flat_map(|file| file.metadata.iter()))
            .find(|(_, value)| value.get("budgetTicketId").is_some());
        let Some((id, _)) = declared else {
            return Ok(());
        };
        Err(miette!(
            help = "delete the key from the template: a travel identity is minted by the run \
                    that first moves the ticket, and the project's ledger is what decides \
                    whether a placed ticket travels fresh.",
            "template '{name}' declares `budgetTicketId` for ticket '{id}'"
        ))
    }

    /// The key is removed unconditionally from every cloned entry, after
    /// re-parenting and after the id-collision check. §FS-rhei-library.5
    fn strip_identity(tasks: &mut BTreeMap<String, YamlValue>) {
        for value in tasks.values_mut() {
            if let YamlValue::Mapping(map) = value {
                map.remove(YamlValue::String("budgetTicketId".into()));
            }
        }
    }

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

    /// The node kinds a rendered template's index declares.
    fn declared_node_kinds(front: &YamlValue) -> Vec<String> {
        front
            .get("structure")
            .and_then(|structure| structure.get("nodeKinds"))
            .and_then(YamlValue::as_sequence)
            .map(|kinds| {
                kinds
                    .iter()
                    .filter_map(|kind| kind.as_str().map(str::to_ascii_lowercase))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn parse_frontmatter(text: &str) -> MietteResult<YamlValue> {
        serde_yaml::from_str(text).map_err(|err| {
            miette!(
                help = "a template's index frontmatter must parse on its own.",
                "failed to parse the template's frontmatter: {err}"
            )
        })
    }

    fn read_text(path: &Path) -> MietteResult<String> {
        fs::read_to_string(path).map_err(|err| file_io_report(path, "failed to read", err))
    }

    /// `rhei instantiate <template> [inputs] --into <rhei>[.<task>]`.
    /// §FS-rhei-library.2
    #[allow(clippy::too_many_arguments)]
    fn instantiate_into_command(
        template: Option<&str>,
        input_args: &[String],
        set_values: &[String],
        set_files: &[String],
        values_files: &[PathBuf],
        target: &str,
        dry_run: bool,
        list_inputs: bool,
    ) -> MietteResult<()> {
        let Some(template) = template else {
            return Err(miette!(
                help = "name the template to place: `rhei instantiate <template> --into <rhei>`.",
                "`--into` needs a template to place"
            ));
        };
        let resolved = resolve_template_reference(template)?;
        let template_dir = resolved.path();
        let mut manifest = load_template_manifest(template_dir)?;
        check_include_cycles(template_dir, &manifest, &mut Vec::new())?;
        manifest.inputs = union_inputs(template_dir, &manifest)?;
        if list_inputs {
            print_template_inputs(&manifest, template);
            return Ok(());
        }

        let host = resolve_union_host(target)?;
        let declaration = resolve_host_machine(&host)?;
        let values = collect_template_inputs(
            &manifest,
            template,
            values_files,
            input_args,
            set_values,
            set_files,
        )?;
        let scratch = tempfile::tempdir().map_err(|err| {
            miette!(
                help = "a template is rendered into a temp directory before it joins a plan. \
                        Check that $TMPDIR exists and is writable.",
                "failed to create the render directory: {err}"
            )
        })?;
        let part_root = scratch.path().join(&manifest.name);
        let part = render_part(template_dir, &manifest, &values, &part_root, None)?;
        let mode = if dry_run { UnionMode::DryRun } else { UnionMode::Place };
        union_into_host(&host, &part, &declaration, mode)
    }

    /// Each combination `--into` refuses is an error naming the pair, rather
    /// than a meaning invented for it. §FS-rhei-library.7.3
    fn refuse_into_combinations(
        output: Option<&Path>,
        execute: bool,
        keep_on_error: bool,
        state_machine: bool,
    ) -> MietteResult<()> {
        let refuse = |flag: &str, why: &str| -> Report {
            miette!(
                help = format!("drop one of them: {why}"),
                "`--into` cannot be combined with `{flag}`"
            )
        };
        if output.is_some() {
            return Err(refuse("--output", "they are two destinations for one instantiation."));
        }
        if execute {
            return Err(refuse(
                "--execute",
                "`--execute` means `rhei run` on a new workspace, and the target may already \
                 be running.",
            ));
        }
        if keep_on_error {
            return Err(refuse(
                "--keep-on-error",
                "there is no staging directory to keep: nothing is written until the union \
                 validates, and on error the target is byte-identical.",
            ));
        }
        if state_machine {
            return Err(refuse(
                "--state-machine",
                "that is an invocation override; a union is a durable write and goes into the \
                 file the target actually runs under.",
            ));
        }
        Ok(())
    }

    /// The identity refusal as it applies to a rendered tree, which is what
    /// `--output` produces. §FS-rhei-library.5
    fn refuse_rendered_identity(name: &str, root: &Path) -> MietteResult<()> {
        let index = root.join("index.rhei.md");
        let plan = root.join("plan.rhei.md");
        let front = [index, plan]
            .into_iter()
            .find(|path| path.is_file())
            .map(|path| read_text(&path))
            .transpose()?
            .and_then(|raw| plan_frontmatter(&raw))
            .and_then(|text| serde_yaml::from_str::<YamlValue>(&text).ok())
            .and_then(|value| task_metadata_entries(&value))
            .unwrap_or_default();
        let tickets = read_part_tickets(root)?;
        refuse_declared_identity(name, &front, &tickets)
    }

    /// `--mount`, `--seam` and `--pass` are gone, and each says what replaced
    /// it: a removed flag that errors with `unexpected argument` teaches
    /// nothing about where composition went.
    // §FS-rhei-library.1
    fn refuse_removed_composition_flags(
        mounts: &[String],
        seams: &[String],
        passes: &[String],
    ) -> MietteResult<()> {
        let named = [("--mount", mounts), ("--seam", seams), ("--pass", passes)]
            .into_iter()
            .filter(|(_, values)| !values.is_empty())
            .map(|(flag, _)| flag)
            .collect::<Vec<_>>();
        if named.is_empty() {
            return Ok(());
        }
        Err(miette!(
            help = "compose by graph union instead:\n  \
                    rhei instantiate <template> [inputs] --into <rhei>[.<task>]\n\
                    and build a template out of templates with `includes:` in its \
                    template.yaml, whose entries may place their tickets `under:` a task of \
                    the host. Names in the result are the names their authors wrote, so \
                    there is nothing to mount, seam or pass.",
            "{} no longer exists: composition is graph union, not a mount-and-seam compile",
            named.join(", ")
        ))
    }

    /// The manifest fields the compiler owned, refused by name for the same
    /// reason its flags are: silently ignoring `ports:` would leave a template
    /// that reads as composed and is not.
    // §FS-rhei-library.1
    fn refuse_removed_manifest_fields(raw: &str, manifest_path: &Path) -> MietteResult<()> {
        const REMOVED: [&str; 8] =
            ["ports", "data", "expose", "use", "bind", "seams", "compatibility", "select"];
        let source: YamlValue = match serde_yaml::from_str(raw) {
            Ok(source) => source,
            // The typed parse above reports a malformed manifest better.
            Err(_) => return Ok(()),
        };
        let declared: Vec<&str> =
            REMOVED.into_iter().filter(|key| source.get(*key).is_some()).collect();
        if declared.is_empty() {
            return Ok(());
        }
        Err(miette!(
            help = "a template's states, edges, profiles, kinds and tickets join a host's by \
                    name, so there is no interface to declare: delete the field, and compose \
                    with `includes:` here or with `rhei instantiate --into <rhei>` at the \
                    call site.",
            "'{}' declares {}, which the block compiler owned and graph union replaced",
            display_path(manifest_path).display(),
            declared.join(", ")
        ))
    }
