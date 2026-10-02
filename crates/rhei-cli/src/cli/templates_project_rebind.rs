    // Laying a project template onto a project that already exists, which
    // rebinds its default machine: the manifest a default must not be laid
    // under, the tickets a replacement would strand, and the refusals that say
    // so before a byte is written.

    // §FS-rhei-library.2.2 §FS-rhei-library.2.3

    /// `rhei instantiate <project-template> --into <project>`: keep the
    /// project's manifest, replace its root `states.yaml`, copy the bundle,
    /// hoist the settings, and lay only the members it does not have yet.
    /// §FS-rhei-library.2.2
    fn lay_project_into(lay: &ProjectLay<'_>, project: &Path) -> MietteResult<()> {
        let rendered = render_project(lay)?;
        refuse_deferring_declaration(project, &rendered.machine)?;
        // Held from before the first read through the last write, as every
        // `--into` is; `--dry-run` writes nothing, not even the sidecar.
        // §FS-rhei-library.2 §FS-rhei-new.4
        let _lock = if lay.dry_run { None } else { Some(lock_new_create(project)?) };
        let root = plan_project_root(&rendered.root, project)?;
        refuse_linked_root_writes(&root.writes, project)?;
        let checked = check_replacement(project, &root, &rendered.machine)?;
        // A directory that exists and is not a rhei is the member-laying
        // path's "already exists", raised before the root is written.
        for member in lay.members {
            let dir = project.join(&member.manifest.name);
            if dir.exists() && !workspace::is_workspace(&dir) {
                return Err(instantiate_output_exists_error(&dir, &member.manifest.name, &[], false));
            }
        }
        let previous = project.join("states.yaml");
        let previous = previous.is_file().then(|| {
            machine_name_of(&fs::read_to_string(&previous).unwrap_or_default())
                .unwrap_or_else(|| "an unnamed machine".to_owned())
        });
        let mut summary = ProjectSummary::new(lay.manifest, &rendered.machine, &root);
        summary.into = Some((previous, checked));

        let backup = (!lay.dry_run).then(|| RootBackup::take(&root.writes, project));
        if let Some(backup) = backup.as_ref() {
            write_project_root(&root.writes, project, project).inspect_err(|_| backup.restore())?;
        }
        let mut laid_dirs = Vec::new();
        for member in lay.members {
            let name = &member.manifest.name;
            let dir = project.join(name);
            // A member is work, so it is laid once and never replaced or
            // unioned into again. §FS-rhei-library.2.2
            if dir.exists() {
                summary.members.push(format!("{name} already exists and was left as it is"));
                continue;
            }
            match lay_project_member(lay, member, project, lay.dry_run) {
                Ok(laid) => {
                    let verb = if lay.dry_run { "would be laid" } else { "laid" };
                    summary.members.push(member_line(name, verb, &laid));
                    laid_dirs.push(dir);
                }
                Err(err) => {
                    if let Some(backup) = backup.as_ref() {
                        for dir in &laid_dirs {
                            let _ = remove_path(dir, false);
                        }
                        backup.restore();
                    }
                    return Err(err);
                }
            }
        }
        summary.print(project, lay.dry_run);
        Ok(())
    }

    /// For as long as the deprecated declaration exists a manifest's own
    /// `**States:** X` resolves ahead of the root file, so a default laid under
    /// another name would govern nothing for a release. Refused before anything
    /// is written; a declaration naming the laid machine is left as it is.
    /// §FS-rhei-library.2.2 §FS-rhei-states-deprecation.1
    fn refuse_deferring_declaration(project: &Path, machine: &str) -> MietteResult<()> {
        let path = project.join(workspace::PANTA_INDEX_FILE);
        let raw = read_text(&path)?;
        let Ok(manifest) = rhei_core::parser::parse_panta_manifest(&raw) else {
            // Loading the project reports a malformed manifest better.
            return Ok(());
        };
        if !manifest.states_declared || manifest.states == machine {
            return Ok(());
        }
        let shown = display_slash(&path);
        Err(miette!(
            help = format!(
                "delete the `**States:**` line from '{shown}' — the declaration is deprecated, \
                 and the states.yaml at the project root is the default without it — then run \
                 this again."
            ),
            "'{shown}' declares `**States:** {}`, so the default this lays, '{machine}', would \
             not govern the project until the declaration is removed",
            manifest.states
        ))
    }

    /// The tickets the replacement default would govern, counted for the
    /// summary's `checked:` line. §FS-rhei-library.2.2
    #[derive(Clone, Copy)]
    struct CheckedTickets {
        tickets: usize,
        rheis: usize,
        basin: bool,
    }

    /// Check a replacement default in a validation copy of the project, before
    /// anything is written.
    ///
    /// Which tickets it reads is resolution's answer: the machine set is built
    /// twice through the one path every command resolves by — as the project
    /// stands, and with the root file replaced — and only the errors the second
    /// introduces are the replacement's. A member whose own declaration
    /// resolves elsewhere is therefore never read, without a rule here saying
    /// so. Then the prospective project is validated in its own terms.
    /// §FS-rhei-library.2.3 §FS-rhei-plan-language.1.3
    fn check_replacement(
        project: &Path,
        root: &ProjectRoot,
        machine: &str,
    ) -> MietteResult<CheckedTickets> {
        let scratch = tempfile::tempdir().map_err(|err| {
            miette!(
                help = "a replacement is checked in a temp directory before it is written. \
                        Check that $TMPDIR exists and is writable.",
                "failed to create the validation directory: {err}"
            )
        })?;
        // Each deprecation is said once, about the real files — settings too, so the
        // whole pass runs here; the copy's paths are gone before anyone reads them.
        // §FS-rhei-templates.1.3
        let _ = validation_pass(project, None);
        let _quiet = ScratchPass::begin();
        // Named after the project, so an id-qualified link still resolves.
        let name = project.file_name().unwrap_or_else(|| std::ffi::OsStr::new("project"));
        let mirror = scratch.path().join(name);
        mirror_project_files(project, &mirror)?;
        // A refusal names the project's files, never the copy it read them in.
        let onto = |report: Report| respell_report(report, &mirror_respellings(&mirror, project));
        let inherited = union_validation_errors(&mirror);
        let before = load_plan_for_validation(&mirror).map_err(onto)?;
        let before_set =
            resolve_state_machines_for_loaded_plan(&mirror, &before, None).map_err(onto)?;
        write_project_root(&root.writes, project, &mirror).map_err(onto)?;
        let after = load_plan_for_validation(&mirror).map_err(onto)?;
        let after_set =
            resolve_state_machines_for_loaded_plan(&mirror, &after, None).map_err(onto)?;

        let introduced = errors_the_replacement_introduces(
            &after.rhei,
            &before_set.validator_set(),
            &after_set.validator_set(),
        );
        let stranding = sort_introduced(&after.rhei, introduced);
        if !stranding.tickets.is_empty() || !stranding.kinds.is_empty() {
            let rebase = |path: &Path| onto_project(path, &mirror, project);
            let kinds: Vec<(String, Vec<(String, PathBuf)>)> = stranding
                .kinds
                .iter()
                .map(|kind| {
                    let lacking = rheis_lacking_kind(&after, &after_set, kind, &mirror)
                        .into_iter()
                        .map(|(rhei, plan)| (rhei, rebase(&plan)))
                        .collect();
                    (kind.clone(), lacking)
                })
                .collect();
            return Err(stranding_refusal(project, machine, &stranding.tickets, &kinds));
        }

        let pass = validation_pass(&mirror, None).map_err(onto)?;
        let introduced = errors_a_rebind_introduces(&inherited, pass.errors);
        if !introduced.is_empty() {
            let (sources, help) = (&pass.state_machine_sources, &pass.help);
            return Err(mirror_validation_report(project, &mirror, sources, &introduced, help));
        }
        Ok(checked_tickets(&after, &after_set))
    }

    /// A path in the validation copy, respelled as the project's own: the copy
    /// is gone before anyone reads the message. Either spelling of the copy
    /// counts, as written and as resolution may have canonicalized it.
    fn onto_project(path: &Path, mirror: &Path, project: &Path) -> PathBuf {
        let resolved = fs::canonicalize(mirror).ok();
        let onto = [Some(mirror), resolved.as_deref()]
            .into_iter()
            .flatten()
            .find_map(|copy| path.strip_prefix(copy).ok())
            .map_or_else(|| path.to_path_buf(), |relative| project.join(relative));
        onto
    }

    /// The validation refusal for an error the replacement introduces, naming
    /// the project and its own files rather than the copy it was found in.
    /// §FS-rhei-library.2.3 §FS-rhei-validate.6
    fn mirror_validation_report(
        project: &Path,
        mirror: &Path,
        sources: &[ValidationMachineSource],
        errors: &[String],
        help: &[String],
    ) -> Report {
        let sources: Vec<ValidationMachineSource> = sources
            .iter()
            .map(|source| ValidationMachineSource {
                path: source.path.as_deref().map(|path| onto_project(path, mirror, project)),
                ..source.clone()
            })
            .collect();
        let report = validation_report(project, &sources, errors, help);
        respell_report(report, &mirror_respellings(mirror, project))
    }

    /// Each spelling of the validation copy a message may use — as written,
    /// as resolution canonicalized it, and as a diagnostic shortens it against
    /// the working directory — paired with the project's own.
    fn mirror_respellings(mirror: &Path, project: &Path) -> Vec<(String, String)> {
        let shown = |path: &Path| path.display().to_string();
        let mut respellings = vec![(shown(mirror), shown(project))];
        if let Ok(resolved) = fs::canonicalize(mirror) {
            respellings.push((shown(&resolved), shown(project)));
        }
        respellings.push((crate::display_path(mirror), crate::display_path(project)));
        respellings.dedup();
        respellings
    }

    /// The errors replacing the default introduces: the project validated under
    /// the machine set it resolves now and under the one it resolves with the
    /// root file replaced, minus what it already carried. A defect the project
    /// already had is the project failing, not the rebind. §FS-rhei-library.2.3
    fn errors_the_replacement_introduces(
        rhei: &rhei_core::ast::Rhei,
        before: &rhei_validator::MachineSet,
        after: &rhei_validator::MachineSet,
    ) -> Vec<String> {
        let inherited = rhei_validator::validate_with_machine_set(rhei, before).errors;
        errors_a_rebind_introduces(&inherited, rhei_validator::validate_with_machine_set(rhei, after).errors)
    }

    /// The errors in `after` that `inherited` does not account for, comparing
    /// a ticket outside its machine by the ticket and the state it holds.
    ///
    /// Both such errors end with the list the machine allows, which is exactly
    /// what a rebind changes: compared whole, a ticket the project already had
    /// in a state no machine of it defines would read as a new defect.
    /// §FS-rhei-library.2.3
    fn errors_a_rebind_introduces(inherited: &[String], after: Vec<String>) -> Vec<String> {
        let mut unspent: Vec<String> = inherited.iter().map(|error| comparable_error(error)).collect();
        after
            .into_iter()
            .filter(|error| match unspent.iter().position(|kept| *kept == comparable_error(error)) {
                Some(index) => {
                    unspent.remove(index);
                    false
                }
                None => true,
            })
            .collect()
    }

    /// An error with the machine's allowed list cut off, when it is one of the
    /// two the validator raises for a ticket's state; any other error whole.
    fn comparable_error(error: &str) -> String {
        const STATE_ERRORS: [(&str, &str); 2] = [
            (" has invalid state '", "'. Allowed: ["),
            (" has state '", "' which is not allowed by its resolved profile."),
        ];
        for (opening, closing) in STATE_ERRORS {
            if let Some((subject, rest)) = error.split_once(opening) {
                if let Some((state, _)) = rest.split_once(closing) {
                    return format!("{subject} holds state '{state}'");
                }
            }
        }
        error.to_owned()
    }

    /// One ticket the replacement would leave in a state its machine does not
    /// allow, with the state it holds.
    #[derive(Debug, PartialEq)]
    struct StrandedTicket {
        id: String,
        state: String,
    }

    /// The introduced errors, sorted into what the refusal names.
    #[derive(Debug, Default)]
    struct Stranding {
        tickets: Vec<StrandedTicket>,
        /// Node kinds the replacement's `node_policy` names that nothing declares.
        kinds: Vec<String>,
    }

    /// Sort introduced errors by the rule that raised them: an authored state
    /// outside its resolved profile, or a `node_policy` kind the structure does
    /// not declare (§FS-rhei-states.9.3). Anything else is left to the full
    /// validation that follows. §FS-rhei-library.2.3
    fn sort_introduced(rhei: &rhei_core::ast::Rhei, introduced: Vec<String>) -> Stranding {
        let tasks = flatten_tasks(rhei);
        let mut stranding = Stranding::default();
        for error in introduced {
            if let Some(kind) = error
                .split_once("references node kind '")
                .and_then(|(_, rest)| rest.split_once('\''))
                .map(|(kind, _)| kind.to_owned())
            {
                if !stranding.kinds.contains(&kind) {
                    stranding.kinds.push(kind);
                }
                continue;
            }
            // The validator names a ticket as `<Kind> <id>`; the subject is
            // rebuilt here rather than parsed out of the sentence.
            let stranded = tasks.iter().find(|task| {
                let subject = format!("{} {} has ", title_case(&task.kind), task.id);
                error
                    .strip_prefix(&subject)
                    .is_some_and(|rest| rest.contains("state '"))
            });
            if let Some(task) = stranded {
                let id = task.id.to_string();
                if !stranding.tickets.iter().any(|ticket| ticket.id == id) {
                    stranding.tickets.push(StrandedTicket { id, state: task.state.clone() });
                }
            }
        }
        stranding
    }

    /// A heading keyword as the validator spells it in a subject.
    fn title_case(kind: &str) -> String {
        let mut chars = kind.chars();
        chars.next().map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
    }

    /// Every rhei the replacement default governs that does not declare `kind`,
    /// with the plan file whose `structure.nodeKinds` would take it. The basin
    /// takes the manifest's structure, and with no such rhei the manifest is
    /// where a later rhei would inherit it from. §FS-rhei-library.2.3
    fn rheis_lacking_kind(
        loaded: &LoadedPlan,
        resolved: &ResolvedMachineSet,
        kind: &str,
        project: &Path,
    ) -> Vec<(String, PathBuf)> {
        let manifest = project.join(workspace::PANTA_INDEX_FILE);
        let mut lacking = Vec::new();
        for rhei in &loaded.rhei_ids {
            if !runs_under_default(resolved, rhei) {
                continue;
            }
            let plan = if rhei == rhei_core::workspace::BASIN_RHEI_ID {
                manifest.clone()
            } else {
                let Some(plan) = loaded.rhei_plans.get(rhei) else { continue };
                plan.clone()
            };
            if !declared_kinds_of(&plan).iter().any(|declared| declared.eq_ignore_ascii_case(kind)) {
                lacking.push((rhei.clone(), plan));
            }
        }
        if lacking.is_empty() {
            lacking.push(("the project".to_owned(), manifest));
        }
        lacking
    }

    /// The node kinds a plan file declares, or the default structure's when it
    /// declares none.
    fn declared_kinds_of(plan: &Path) -> Vec<String> {
        let declared = fs::read_to_string(plan)
            .ok()
            .and_then(|raw| plan_frontmatter(&raw))
            .and_then(|text| serde_yaml::from_str::<YamlValue>(&text).ok())
            .map(|front| declared_node_kinds(&front))
            .unwrap_or_default();
        if declared.is_empty() {
            return rhei_core::ast::Structure::default().node_kinds;
        }
        declared
    }

    /// The refusal for a replacement that strands tickets or node kinds, naming
    /// each with what to change and saying that nothing was written.
    /// §FS-rhei-library.2.3
    fn stranding_refusal(
        project: &Path,
        machine: &str,
        tickets: &[StrandedTicket],
        kinds: &[(String, Vec<(String, PathBuf)>)],
    ) -> Report {
        let mut counted = Vec::new();
        if !tickets.is_empty() {
            counted.push(plural(tickets.len(), "ticket"));
        }
        if !kinds.is_empty() {
            counted.push(plural(kinds.len(), "node kind"));
        }
        let mut body = String::new();
        if !tickets.is_empty() {
            body.push_str(&format!(
                "\n\nthe project default becomes '{machine}'; these tickets run under the \
                 default and hold a state it does not allow them:\n"
            ));
            let width = tickets.iter().map(|ticket| ticket.id.len()).max().unwrap_or(0);
            for ticket in tickets {
                let basin = ticket.id.split('.').next() == Some(rhei_core::workspace::BASIN_RHEI_ID);
                let note = if basin { "   (the basin runs under the project default)" } else { "" };
                body.push_str(&format!("\n  {:<width$}   {}{note}", ticket.id, ticket.state));
            }
        }
        for (kind, lacking) in kinds {
            body.push_str(&format!(
                "\n\n'{machine}' names node kind '{kind}' in its node_policy, and the rheis it \
                 would govern do not declare it:\n"
            ));
            for (rhei, plan) in lacking {
                body.push_str(&format!(
                    "\n  {rhei}: add `{kind}` to structure.nodeKinds in {}",
                    display_slash(plan)
                ));
            }
        }
        let mut remedy = Vec::new();
        if !tickets.is_empty() {
            remedy.push("move each ticket to a state the new machine has");
        }
        if !kinds.is_empty() {
            remedy.push("declare each node kind where it is listed");
        }
        miette!(
            help = format!(
                "{}, then run this again.\nNothing was written: the project is byte-identical to \
                 what it was.",
                remedy.join(", and ")
            ),
            "replacing the default machine of '{}' would strand {}{body}",
            project_label(project),
            counted.join(" and ")
        )
    }

    /// Whether the project default governs `rhei`, read off resolution: it has
    /// no machine of its own, or its own root's file is the default's — a
    /// single-file rhei beside the manifest shares the project root.
    /// §FS-rhei-plan-language.1.3 §AR-rhei-panta.4
    fn runs_under_default(resolved: &ResolvedMachineSet, rhei: &str) -> bool {
        match resolved.per_rhei.get(rhei) {
            None => true,
            Some(own) => own.path.is_some() && own.path == resolved.default.path,
        }
    }

    /// The project as a message names it: its path from the working directory,
    /// or its own name when that path is `.`.
    fn project_label(project: &Path) -> String {
        let shown = display_slash(project);
        match project.file_name() {
            Some(name) if shown == "." => name.to_string_lossy().into_owned(),
            _ => shown,
        }
    }

    fn plural(count: usize, noun: &str) -> String {
        if count == 1 { format!("1 {noun}") } else { format!("{count} {noun}s") }
    }

    /// What the replacement default governs once laid: every ticket of a rhei
    /// with no machine of its own, and the basin's. §FS-rhei-library.2.2
    fn checked_tickets(loaded: &LoadedPlan, resolved: &ResolvedMachineSet) -> CheckedTickets {
        let governed = |rhei: &str| runs_under_default(resolved, rhei);
        let tickets = flatten_tasks(&loaded.rhei)
            .into_iter()
            .filter(|task| task.id.to_string().split('.').next().is_some_and(governed))
            .count();
        let basin = loaded.rhei_ids.iter().any(|rhei| rhei == rhei_core::workspace::BASIN_RHEI_ID);
        let rheis = loaded
            .rhei_ids
            .iter()
            .filter(|rhei| *rhei != rhei_core::workspace::BASIN_RHEI_ID && governed(rhei))
            .count();
        CheckedTickets { tickets, rheis, basin }
    }
