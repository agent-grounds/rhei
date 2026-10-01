// The one-release window: the `**States:**` declaration and the unique-`name`
// match across rhei roots still resolve, still win wherever they resolve, and
// warn where they disagree with the resolution that replaces them.

// §FS-rhei-states-deprecation

// This file is the whole of the window on the code side, so the removal
// release deletes it and the two calls into it, and leaves
// §FS-rhei-plan-language.1.3's three clauses untouched.

// §FS-rhei-states-deprecation.4

/// Where the deprecated pass found a machine. The window owes a different
/// message for each disagreement, and silence where there is none.
/// §FS-rhei-states-deprecation.2
enum DeprecatedSource {
    /// A `states.yaml` in another rhei's execution root — the cross-root
    /// `name:` match. §FS-rhei-states-deprecation.2.3
    CrossRoot(PathBuf),
    /// The declaring rhei's own root, the project root, the already-resolved
    /// project default, or the built-in machine: anywhere but another rhei's
    /// root. Whether this disagrees with the new clauses is decided by the
    /// file sitting in the rhei's own root, not by which of these it was.
    OwnPlace,
}

/// What the deprecated pass resolved, and from where.
/// §FS-rhei-states-deprecation.1
struct DeprecatedResolution {
    resolved: ResolvedStateMachine,
    source: DeprecatedSource,
}

/// Who carries the declaration being resolved. The subject of a warning is the
/// declaration rather than the lookup, so a manifest declaration five rheis
/// inherit warns once and names the manifest.
/// §FS-rhei-states-deprecation.3
enum DeclarationSubject<'a> {
    /// The top-level plan's own declaration, naming the project default: a
    /// project's `index.panta.md`, a Directory Workspace's `index.rhei.md`, or
    /// a lone plan itself. `file` is the one that carries the line.
    Manifest { root: &'a Path, file: &'a Path },
    /// A rhei's own `**States:**` line.
    Rhei { id: &'a str, root: &'a Path },
}

impl DeclarationSubject<'_> {
    /// The once-per-process key. It is the declaration that is the subject, so
    /// a rhei's id is part of its key: two single-file members of one project
    /// share an execution root and carry two distinct declarations.
    /// §FS-rhei-states-deprecation.3
    fn key(&self) -> String {
        match self {
            Self::Manifest { root, .. } => {
                format!("states-declaration:manifest:{}", root.display())
            }
            Self::Rhei { id, root } => {
                format!("states-declaration:rhei:{}:{id}", root.display())
            }
        }
    }

    /// How the warning names whoever carries the line — the file for a
    /// top-level declaration, because a lone plan and a Directory Workspace
    /// have no `index.panta.md` to be told to edit.
    /// §FS-rhei-states-deprecation.2
    fn names_itself(&self) -> String {
        match self {
            Self::Manifest { file, .. } => format!("'{}'", file.display()),
            Self::Rhei { id, .. } => format!("rhei '{id}'"),
        }
    }

    /// The execution root whose `states.yaml` §FS-rhei-plan-language.1.3
    /// clause 1 would read. For a top-level declaration that is the directory
    /// holding the file, which is also what clause 2 reads.
    fn root(&self) -> &Path {
        match self {
            Self::Manifest { root, .. } | Self::Rhei { root, .. } => root,
        }
    }
}

/// The file that actually carries a top-level `**States:**` declaration:
/// `index.panta.md` for a project, `index.rhei.md` for a Directory Workspace,
/// and the plan itself for a lone plan. Naming a file the tree does not hold
/// tells the reader to delete a line from nowhere.
// §FS-rhei-states-deprecation.2
fn declaring_plan_file(input: &Path) -> PathBuf {
    if let Some(project_dir) = workspace::panta_project_dir(input) {
        return project_dir.join(workspace::PANTA_INDEX_FILE);
    }
    if let Some(workspace_dir) = workspace::workspace_dir(input) {
        return workspace_dir.join(workspace::RHEI_INDEX_FILE);
    }
    input.to_path_buf()
}

/// The deprecated resolution of one rhei's own `**States:**` line, exactly as
/// the previous release resolved it: the rhei's own execution root for a
/// custom same-name declaration, then the already-resolved project default,
/// then a unique `name:` match among the project's candidate roots, then the
/// built-in machine for a declaration that names it.
///
/// `Ok(None)` is "found nothing", which is what lets
/// §FS-rhei-plan-language.1.3 clause 1 run behind it; the previous release
/// raised the missing-definition error here instead, and that error now lives
/// with the clauses it survives into. Several candidates is still the
/// ambiguity error, unchanged.
// §FS-rhei-states-deprecation.1
fn resolve_declared_rhei_machine(
    input: &Path,
    loaded: &LoadedPlan,
    rhei_id: &str,
    machine_name: &str,
    default: &ResolvedStateMachine,
) -> MietteResult<Option<DeprecatedResolution>> {
    let builtin_name = &rhei_validator::StateMachine::builtin_default().name;
    let restates_builtin_default =
        machine_name == default.machine.name && machine_name == builtin_name;
    if !restates_builtin_default {
        // Restating the built-in default stays equivalent to omission in
        // every respect (§AR-rhei-panta.4): only a custom same-name
        // declaration gives its own candidate first refusal.
        if let Some(root) = loaded.rhei_roots.get(rhei_id) {
            let candidate = root.join("states.yaml");
            if candidate.is_file() {
                // An explicit declaration always gives its own candidate first
                // refusal, even when it repeats the default name.
                let machine = load_state_machine(Some(&candidate))?;
                if machine.name == machine_name {
                    return Ok(Some(DeprecatedResolution {
                        resolved: ResolvedStateMachine { machine, path: Some(candidate) },
                        source: DeprecatedSource::OwnPlace,
                    }));
                }
            }
        }
    }
    if machine_name == default.machine.name {
        return Ok(Some(DeprecatedResolution {
            resolved: default.clone(),
            source: DeprecatedSource::OwnPlace,
        }));
    }

    let own_root = loaded.rhei_roots.get(rhei_id);
    let candidates = declared_machine_candidates(input, loaded.rhei_roots.values());
    let matches = candidates_declaring(&candidates, machine_name)?;

    // An explicit built-in name falls back only after the matching-file lookup
    // finds nothing.
    if matches.is_empty() {
        let builtin = rhei_validator::StateMachine::builtin_default();
        if machine_name == builtin.name {
            return Ok(Some(DeprecatedResolution {
                resolved: ResolvedStateMachine { machine: builtin, path: None },
                source: DeprecatedSource::OwnPlace,
            }));
        }
        return Ok(None);
    }
    if matches.len() > 1 {
        return Err(miette!(
            help = states_declaration_help(),
            "rhei '{rhei_id}' declares state machine '{machine_name}', and more than one \
             root holds a states file declaring it: {}.\nMove the definitive file to the \
             rhei's own root or pass --state-machine <path>.",
            quoted_paths(&matches)
        ));
    }
    let (path, machine) = matches.into_iter().next().expect("single match");
    // The project root is where a default belongs; only another *rhei's* root
    // is the crossing the window is closing. §FS-rhei-states-deprecation.2.3
    let crossed = own_root.map_or(true, |root| path != root.join("states.yaml"))
        && path != auto_state_machine_path(input);
    Ok(Some(DeprecatedResolution {
        resolved: ResolvedStateMachine { machine, path: Some(path.clone()) },
        source: if crossed {
            DeprecatedSource::CrossRoot(path)
        } else {
            DeprecatedSource::OwnPlace
        },
    }))
}

/// The deprecated resolution of `index.panta.md`'s own declaration — the
/// project default — as the previous release resolved it: the project root's
/// `states.yaml` when it declares the name, then a unique `name:` match among
/// the rhei roots, then the built-in machine for a declaration that names it.
///
/// `Ok(None)` is "found nothing"; the error for a declaration nothing supplies
/// is raised by the clauses behind this pass, which keep it.
// §FS-rhei-states-deprecation.1
fn resolve_declared_project_default(
    input: &Path,
    loaded: &LoadedPlan,
    declared_name: &str,
) -> MietteResult<Option<DeprecatedResolution>> {
    let builtin = rhei_validator::StateMachine::builtin_default();
    let candidate = auto_state_machine_path(input);
    if candidate.is_file() {
        let machine = load_state_machine(Some(&candidate))?;
        if machine.name == declared_name {
            return Ok(Some(DeprecatedResolution {
                resolved: ResolvedStateMachine { machine, path: Some(candidate) },
                source: DeprecatedSource::OwnPlace,
            }));
        }
    }
    if declared_name == builtin.name {
        return Ok(Some(DeprecatedResolution {
            resolved: ResolvedStateMachine { machine: builtin, path: None },
            source: DeprecatedSource::OwnPlace,
        }));
    }

    // When the project-root file is absent or mismatched, a *unique* `name:`
    // match in a rhei root resolves the machine file; several matches are
    // ambiguous — a stale copy must not win silently. §AR-rhei-panta.4
    let roots = sorted_task_roots(loaded);
    let candidates: Vec<PathBuf> = roots
        .into_iter()
        .map(|root| root.join("states.yaml"))
        .filter(|path| *path != candidate)
        .collect();
    let matches = candidates_declaring(&candidates, declared_name)?;
    if matches.len() > 1 {
        return Err(miette!(
            help = states_declaration_help(),
            "plan declares state machine '{declared_name}', and more than one rhei root holds \
             a states file declaring it: {}.\nMove the definitive file to the project \
             root or pass --state-machine <path>.",
            quoted_paths(&matches)
        ));
    }
    let Some((path, machine)) = matches.into_iter().next() else {
        return Ok(None);
    };
    Ok(Some(DeprecatedResolution {
        resolved: ResolvedStateMachine { machine, path: Some(path.clone()) },
        source: DeprecatedSource::CrossRoot(path),
    }))
}

/// The one flat candidate set a rhei's `**States:**` line is resolved against:
/// the project root's `states.yaml` and every rhei execution root's, in a
/// single list rather than in sequence. Two files among them declaring the name
/// are therefore the ambiguity error wherever they sit, and the project root is
/// one of the roots that can be ambiguous rather than a step before them.
///
/// It is a function because `rhei viz`'s static mirror resolves the same
/// declarations off the same tree, and a mirror that enumerates its own set is
/// a second answer to the same question. §FS-rhei-states-deprecation.1
fn declared_machine_candidates<'a>(
    input: &Path,
    rhei_roots: impl IntoIterator<Item = &'a PathBuf>,
) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = rhei_roots.into_iter().cloned().collect();
    roots.sort();
    roots.dedup();
    let mut candidates = vec![auto_state_machine_path(input)];
    candidates.extend(roots.into_iter().map(|root| root.join("states.yaml")));
    candidates
}

/// Every distinct candidate that loads and declares `machine_name`, with the
/// loader injected so that the viz mirror counts this set rather than one of
/// its own. An unloadable candidate is a real project problem; swallowing it
/// here would surface as a misleading "not found" instead. §AR-rhei-panta.4
fn declaring_candidates<E>(
    candidates: &[PathBuf],
    machine_name: &str,
    load: impl Fn(&Path) -> Result<rhei_validator::StateMachine, E>,
) -> Result<Vec<(PathBuf, rhei_validator::StateMachine)>, E> {
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut matches = Vec::new();
    for candidate in candidates {
        if !seen.insert(candidate.clone()) || !candidate.is_file() {
            continue;
        }
        let machine = load(candidate)?;
        if machine.name == machine_name {
            matches.push((candidate.clone(), machine));
        }
    }
    Ok(matches)
}

/// [`declaring_candidates`] under this pass's own loader, which reports an
/// unreadable candidate the way every other `miette` diagnostic reports one.
// §AR-rhei-panta.4
fn candidates_declaring(
    candidates: &[PathBuf],
    machine_name: &str,
) -> MietteResult<Vec<(PathBuf, rhei_validator::StateMachine)>> {
    declaring_candidates(candidates, machine_name, |candidate| {
        load_state_machine(Some(candidate))
    })
}

fn quoted_paths(matches: &[(PathBuf, rhei_validator::StateMachine)]) -> String {
    matches
        .iter()
        .map(|(path, _)| format!("'{}'", path.display()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Say what the window owes about one declaration that resolved, and nothing
/// where the deprecated mechanisms and §FS-rhei-plan-language.1.3 agree —
/// which is the shape every instantiated template ships.
///
/// At most one line per declaration: a cross-root match that also leaves a
/// file unread in the rhei's own root prints the crossing, because that is the
/// one naming the file which actually resolved.
// §FS-rhei-states-deprecation.2
fn warn_about_deprecated_resolution(
    subject: &DeclarationSubject<'_>,
    declared_name: &str,
    found: &DeprecatedResolution,
) {
    if let DeprecatedSource::CrossRoot(path) = &found.source {
        warn_cross_root_match(subject, declared_name, path);
        return;
    }
    let own = subject.root().join("states.yaml");
    if own.is_file() && Some(own.as_path()) != found.resolved.path.as_deref() {
        warn_own_root_file_deferred(subject, &found.resolved, &own);
    }
}

/// The declaration resolved nothing and clause 1 read the rhei's own file
/// instead. The previous release failed this tree.
/// §FS-rhei-states-deprecation.2.1
fn warn_declaration_nothing_supplies(
    subject: &DeclarationSubject<'_>,
    declared_name: &str,
    resolved_from: &Path,
    resolved_name: &str,
) {
    // "beside it" is true of all three top-level shapes; "at the project root"
    // was only true of one of them. §FS-rhei-states-deprecation.2
    let place = match subject {
        DeclarationSubject::Manifest { .. } => "beside it",
        DeclarationSubject::Rhei { .. } => "in its own root",
    };
    say_once(
        subject,
        format!(
            "{} declares state machine '{declared_name}', which no states file declares; \
             '{}' ({resolved_name}) {place} resolves instead. The `**States:**` declaration \
             is deprecated and is removed in the next release — delete the line.",
            subject.names_itself(),
            resolved_from.display(),
        ),
    );
}

/// The declaration still resolves, so the file in the rhei's own root waits a
/// release. This is how a tree that would otherwise change machines silently
/// next release says so now. §FS-rhei-states-deprecation.2.2
fn warn_own_root_file_deferred(
    subject: &DeclarationSubject<'_>,
    resolved: &ResolvedStateMachine,
    deferred: &Path,
) {
    // Read leniently: the warning is beside a resolution that already
    // succeeded, and a file the window only mentions must not fail the run.
    let deferred_name = load_state_machine(Some(deferred))
        .map(|machine| format!(" ({})", machine.name))
        .unwrap_or_default();
    let running = &resolved.machine.name;
    say_once(
        subject,
        format!(
            "{} runs under '{running}' because its deprecated `**States:**` declaration still \
             resolves; '{}'{deferred_name} in its own root takes over in the next release. \
             Delete the line to move now, or remove the file to stay on '{running}'.",
            subject.names_itself(),
            deferred.display(),
        ),
    );
}

/// The declaration resolved from a `states.yaml` in a root that is not its
/// own. The remedy names where the file belongs instead.
/// §FS-rhei-states-deprecation.2.3
fn warn_cross_root_match(
    subject: &DeclarationSubject<'_>,
    declared_name: &str,
    resolved_from: &Path,
) {
    let remedy = match subject {
        // There is no declaring rhei root to move the file to: the project
        // root is the single remedy.
        DeclarationSubject::Manifest { .. } => "move the file to the project root".to_owned(),
        DeclarationSubject::Rhei { root, .. } => format!(
            "move the file to '{}', or to the project root to make it the default",
            root.join("states.yaml").display()
        ),
    };
    let what = match subject {
        DeclarationSubject::Manifest { .. } => {
            format!("declares state machine '{declared_name}' as the project default")
        }
        DeclarationSubject::Rhei { .. } => format!("declares state machine '{declared_name}'"),
    };
    say_once(
        subject,
        format!(
            "{} {what}, resolved from '{}' in another rhei's root. Resolution across rhei \
             roots is deprecated and is removed in the next release — {remedy}.",
            subject.names_itself(),
            resolved_from.display(),
        ),
    );
}

/// On stderr, once per declaration per process, and never over a candidate
/// list — §FS-rhei-templates.1.3's contract, through the machinery that
/// already serves the other deprecation rather than a second copy of it.
/// §FS-rhei-states-deprecation.3
fn say_once(subject: &DeclarationSubject<'_>, message: String) {
    if !claim_deprecation_warning(&subject.key()) {
        return;
    }
    eprintln!("warning: {message}");
}

#[cfg(test)]
mod deprecated_pass_tests {
    use super::*;

    /// The seam the window turns on: a declaration naming a machine nothing
    /// supplies returns "found nothing" rather than erroring, so clause 1 runs
    /// behind it. The previous release raised the missing-definition error
    /// here. §FS-rhei-states-deprecation.1
    #[test]
    fn a_declaration_nothing_supplies_returns_found_nothing() {
        let temp = tempfile::tempdir().expect("create the fixture");
        let root = temp.path().join("project");
        let member = root.join("a-ticket");
        std::fs::create_dir_all(member.join("tasks")).expect("create the member");
        std::fs::write(root.join("index.panta.md"), "# Panta: Seam\n").expect("write manifest");
        std::fs::write(member.join("index.rhei.md"), "# Rhei: a-ticket\n**States:** absent\n")
            .expect("write index");
        std::fs::write(member.join("tasks/01.md"), "### Task 1: One\n**State:** pending\n")
            .expect("write task");
        std::fs::write(member.join("states.yaml"), "name: local\nversion: 1\nstates:\n  pending:\n    initial: true\n    description: One\n  done:\n    final: true\n    description: Two\ntransitions:\n  - from: pending\n    to: done\n")
            .expect("write the member machine");

        let loaded = load_plan(&root).expect("the project loads");
        let default = ResolvedStateMachine {
            machine: rhei_validator::StateMachine::builtin_default(),
            path: None,
        };
        let found =
            resolve_declared_rhei_machine(&root, &loaded, "a-ticket", "absent", &default)
                .expect("the deprecated pass must not error on a declaration nothing supplies");
        assert!(
            found.is_none(),
            "the pass owes clause 1 a 'found nothing', not a machine"
        );
    }

    /// The clause order behind that seam: the rhei's own root outranks the
    /// project root, and the project root outranks the built-in machine.
    /// §FS-rhei-plan-language.1.3
    #[test]
    fn clause_one_outranks_clause_two_which_outranks_the_builtin() {
        let temp = tempfile::tempdir().expect("create the fixture");
        let root = temp.path().join("project");
        let member = root.join("m1");
        std::fs::create_dir_all(member.join("tasks")).expect("create the member");
        std::fs::write(root.join("index.panta.md"), "# Panta: Order\n").expect("write manifest");
        std::fs::write(member.join("index.rhei.md"), "# Rhei: m1\n").expect("write index");
        std::fs::write(member.join("tasks/01.md"), "### Task 1: One\n**State:** pending\n")
            .expect("write task");
        let yaml = |name: &str| {
            format!("name: {name}\nversion: 1\nstates:\n  pending:\n    initial: true\n    description: One\n  done:\n    final: true\n    description: Two\ntransitions:\n  - from: pending\n    to: done\n")
        };

        // Clause 3: nothing placed anywhere.
        let loaded = load_plan(&root).expect("the project loads");
        let resolved = resolve_state_machines_for_loaded_plan(&root, &loaded, None)
            .expect("resolution succeeds with no file placed");
        assert_eq!(
            resolved.default.machine.name,
            rhei_validator::StateMachine::builtin_default().name
        );
        assert!(!resolved.per_rhei.contains_key("m1"), "clause 3 leaves the member on the default");

        // Clause 2: a project-root file the manifest does not declare.
        std::fs::write(root.join("states.yaml"), yaml("at-the-project-root"))
            .expect("write the project machine");
        let loaded = load_plan(&root).expect("the project loads");
        let resolved = resolve_state_machines_for_loaded_plan(&root, &loaded, None)
            .expect("resolution succeeds with the project file placed");
        assert_eq!(resolved.default.machine.name, "at-the-project-root");
        assert!(
            !resolved.per_rhei.contains_key("m1"),
            "clause 2 leaves the member on the project default"
        );

        // Clause 1: the member's own root wins over both.
        std::fs::write(member.join("states.yaml"), yaml("at-the-rheis-own-root"))
            .expect("write the member machine");
        let loaded = load_plan(&root).expect("the project loads");
        let resolved = resolve_state_machines_for_loaded_plan(&root, &loaded, None)
            .expect("resolution succeeds with both files placed");
        assert_eq!(resolved.default.machine.name, "at-the-project-root");
        assert_eq!(
            resolved.per_rhei.get("m1").map(|found| found.machine.name.as_str()),
            Some("at-the-rheis-own-root"),
            "clause 1 outranks clause 2 for the rhei whose root holds the file"
        );
    }
}
