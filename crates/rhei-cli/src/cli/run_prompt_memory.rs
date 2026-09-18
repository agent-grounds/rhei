// The shared floor under the mid-term memory sections: what the project graph
// hands prompt composition beyond one invocation, the caps every section obeys,
// and the four primitives the renderers next door are built from — a path, a
// truncation, a one-line summary, and a ledger read.
//
// Its own part because Position, Plan History, Previous Visits, and the
// navigation block each render a different slice of the same memory; the slice
// is theirs, the reading of it is shared.

// §AR-source-file-size.3 §FS-rhei-memory.4

/// Everything the memory sections read that the invocation itself does not
/// carry: the project, its rheis, and where each one's runtime lives.
///
/// Composition is a pure function of this plus the invocation, so the struct
/// holds data and never a handle to anything that varies per run.
// §FS-rhei-memory.1.2 §FS-rhei-memory.4.1
struct PromptMemory {
    /// Title of the Panta — the manifest's for a project, the rhei's own for a
    /// bare rhei's implicit Panta.
    panta_title: String,
    /// True when an `index.panta.md` was actually authored. A bare rhei has no
    /// `### Project Context`. §FS-rhei-memory.3.1
    explicit_panta: bool,
    /// The project manifest, when there is one.
    panta_manifest: Option<PathBuf>,
    /// Rhei ids in load order — the order `### Reading the rhei` lists them in.
    rhei_ids: Vec<String>,
    rhei_roots: HashMap<String, PathBuf>,
    rhei_titles: HashMap<String, String>,
    rhei_plans: HashMap<String, PathBuf>,
    /// Content sections of the merged graph, in authored order.
    content_sections: Vec<rhei_core::ast::ContentSection>,
    /// File that defines each ticket, keyed by qualified id.
    task_sources: HashMap<String, PathBuf>,
    /// The `runtime/` directory `rhei run` names agent logs under.
    // §FS-rhei-agents.8.1
    runtime_dir: PathBuf,
    /// Tickets the current `rhei run` pass has spawned and not yet reaped.
    /// `rhei run` writes no `**Assignee:**` on claim, so this is the only
    /// witness that another agent of this pass is working. §FS-rhei-memory.4.3
    run_in_flight: BTreeSet<String>,
    /// Whether this surface pastes the ticket's own input sections —
    /// `## Prior Task Results` and `## Child Task Results`.
    ///
    /// `see above` is a claim about *this* prompt, and `rhei next` prints
    /// neither section, so on that surface a deferred summary would point at
    /// nothing. `## Checkpoints` is rendered by both and is not covered here.
    // §FS-rhei-memory.4.3 §FS-rhei-supervision.3.4
    pastes_task_inputs: bool,
    /// Whether every memory path renders absolute rather than against a root.
    ///
    /// A relative path is only readable against an anchor the reader has.
    /// `rhei run` gives its agent one in the prompt, plus a cwd inside the
    /// checkout, so it anchors there; `rhei next` exports nothing and
    /// promises no cwd, so on that surface every path is absolute or it is a
    /// guess. One flag, because a prompt that mixed the two forms would be
    /// worse than either.
    // §FS-rhei-memory.3.4
    absolute_paths: bool,
    /// The task's resolved negative source policy. Empty preserves legacy
    /// composition byte-for-byte. §FS-rhei-memory.1.2 §FS-rhei-memory.4.1
    exclusions: ResolvedExclusions,
    /// True only when the selected profile declares a denial adapter. Manual
    /// `rhei next` output always leaves this false. §FS-rhei-agents.3
    exclusions_filesystem_denied: bool,
}

/// Every cap §FS-rhei-memory.4 states, in one place, so a reader can check the
/// implementation against the spec without reading four renderers.
// §FS-rhei-memory.4.5
mod memory_caps {
    /// §FS-rhei-memory.4.2: siblings listed under `### Siblings`.
    pub const SIBLINGS: usize = 30;
    /// §FS-rhei-memory.4.2: lines of the parent's pasted body.
    pub const PARENT_BODY_LINES: usize = 200;
    /// §FS-rhei-memory.4.2: lines of each pasted content-section block.
    pub const CONTEXT_LINES: usize = 1000;
    /// §FS-rhei-memory.4.3: lines of `## Plan History`.
    pub const PLAN_HISTORY: usize = 40;
    /// §FS-rhei-memory.4.3: entries under `### In Flight`.
    pub const IN_FLIGHT: usize = 20;
    /// §FS-rhei-memory.4.3: entries under `### Dependents`.
    pub const DEPENDENTS: usize = 30;
    /// §FS-rhei-memory.4.4: trailing lines of the pasted result file.
    pub const RESULT_LINES: usize = 100;
    /// §FS-rhei-memory.4.3: columns a Plan History summary is cut to.
    pub const SUMMARY_COLUMNS: usize = 120;
}

/// Build the memory a prompt composes from, off an already-loaded plan.
// §FS-rhei-memory.4.1
fn prompt_memory(
    loaded: &LoadedPlan,
    input: &Path,
    runtime_dir: &Path,
    run_in_flight: BTreeSet<String>,
) -> PromptMemory {
    let panta_manifest = rhei_core::workspace::panta_project_dir(input)
        .map(|dir| dir.join(rhei_core::workspace::PANTA_INDEX_FILE));
    PromptMemory {
        panta_title: loaded.rhei.title.clone(),
        explicit_panta: loaded.is_panta_project(),
        panta_manifest,
        rhei_ids: loaded.rhei_ids.clone(),
        rhei_roots: loaded.rhei_roots.clone(),
        rhei_titles: loaded.rhei_titles.clone(),
        rhei_plans: loaded.rhei_plans.clone(),
        content_sections: loaded.rhei.content_sections.clone(),
        task_sources: loaded.task_sources.clone(),
        runtime_dir: runtime_dir.to_path_buf(),
        run_in_flight,
        pastes_task_inputs: true,
        absolute_paths: false,
        exclusions: ResolvedExclusions::default(),
        exclusions_filesystem_denied: false,
    }
}

/// The owning rhei's id, as the qualification prefix spells it.
// §FS-rhei-memory.4.1
fn owning_rhei_id(render_context: &RuntimeTemplateContext<'_>) -> Option<String> {
    rhei_id_of(render_context.task)
}

/// How one ticket is named in a memory section: its kind, title-cased, and its
/// qualified id — the form `## Child Tasks` and `rhei list` already print.
// §FS-rhei-memory.3.1 §FS-rhei-memory.3.2 §FS-rhei-plan-language.3.7
fn memory_node_label(task: &rhei_core::ast::Task) -> String {
    format!("{} {}", title_case_kind(&task.kind), task.id)
}

/// The state one ticket renders as: the machine's name for it.
///
/// A `**State:**` line may carry a counted-loop suffix (`work-3`), which is
/// bookkeeping for the engine and noise to a reader — and the invocation's own
/// state is already normalized on the same line, so leaving a sibling's raw
/// made the two look like different states of different machines.
// §FS-rhei-memory.4.5 §FS-rhei-plan-language.3.2
fn memory_state_name(
    task: &rhei_core::ast::Task,
    machine: &rhei_validator::StateMachine,
) -> String {
    normalized_state_name(task.state.as_str(), machine)
}

/// Every task of the merged graph, in plan order, parents before children.
// §FS-rhei-plan-language.1.2
fn flatten_task_slice(tasks: &[rhei_core::ast::Task]) -> Vec<&rhei_core::ast::Task> {
    fn collect<'a>(task: &'a rhei_core::ast::Task, out: &mut Vec<&'a rhei_core::ast::Task>) {
        out.push(task);
        for child in &task.children {
            collect(child, out);
        }
    }
    let mut out = Vec::new();
    for task in tasks {
        collect(task, &mut out);
    }
    out
}

/// Whether a ticket's authored state is terminal under its machine.
fn task_state_is_terminal(
    task: &rhei_core::ast::Task,
    machine: &rhei_validator::StateMachine,
) -> bool {
    is_terminal_state(&memory_state_name(task, machine), machine)
}

/// Render a filesystem path the way `{output.<name>.path}` renders one:
/// relative to the prompt's execution root, absolute once the checkout is elsewhere.
/// else. A path outside the root has no relative form and stays absolute.
///
/// Every memory path in a prompt passes through here, so a surface that
/// anchors nothing (`absolute_paths`) resolves all of them the same way — the
/// one place where the anchor of a whole prompt is decided.
// §FS-rhei-memory.3.4 §FS-rhei-states.4
fn memory_path(render_context: &RuntimeTemplateContext<'_>, path: &Path) -> String {
    let root = render_context.workspace_root;
    let placed = placed_under_artifact_root(root, path);
    let absolute = placed
        .as_ref()
        .map(|(_, absolute)| absolute.clone())
        .unwrap_or_else(|| path.to_path_buf());
    if render_context.memory.is_some_and(|memory| memory.absolute_paths) {
        return absolute_memory_path(&absolute);
    }
    if !roots_are_one_directory(render_context.checkout_root, root) {
        return spelled_path(&absolute);
    }
    match placed.as_ref().map(|(relative, _)| relative.as_path()) {
        Some(relative) if relative.as_os_str().is_empty() => ".".to_string(),
        Some(relative) => spelled_path(relative),
        None => spelled_path(&absolute),
    }
}

/// Where `path` sits under the artifact root when only their resolved forms
/// say so — `None` when it sits outside it either way.
///
/// One prompt, one spelling. Paths reach composition two ways: joined onto the
/// artifact root exactly as the caller typed it, and canonicalized on the way
/// to a callback. Where the root is reached through a symlink or under an 8.3
/// short name those are two strings for one directory, and a worker told its
/// plan is in one and its own result file in the other cannot tell they are
/// the same place. Matching canonically and re-joining onto `root` gives the
/// prompt back the one spelling it names the root by.
///
/// Only [`placed_under_artifact_root`] may ask this, and only after the
/// textual question, whose answer this one cannot improve on.
// §FS-rhei-memory.1.2 §FS-rhei-agents.4.1
fn under_artifact_root(root: &Path, path: &Path) -> Option<PathBuf> {
    let root = canonical_spelling(root)?;
    Some(canonical_spelling(path)?.strip_prefix(&root).ok()?.to_path_buf())
}

/// `root` and a path under it, as one path. The root itself arrives here as an
/// empty relative form, which `join` would spell with a trailing separator.
fn join_under_root(root: &Path, relative: &Path) -> PathBuf {
    if relative.as_os_str().is_empty() { root.to_path_buf() } else { root.join(relative) }
}

/// Where `path` sits under the artifact root, as the pair every prompt path is
/// rendered from: its form relative to the root, and its absolute form.
///
/// Textually first. A path already written under the root's own spelling is
/// already both answers, and taking them is the only way to keep what the
/// caller wrote: `Path::join` writes the platform's separator, so re-deriving
/// `states.yaml` under a root spelled `C:/Users/x/ws` hands the prompt
/// `C:/Users/x/ws\states.yaml` — a path that resolves and that nobody typed,
/// which is the complaint the rule exists to answer. The resolved match is
/// asked only of a path that reaches the root by a *different* spelling, where
/// the rejoin costs nothing because there was no spelling of it to keep.
// §FS-rhei-memory.1.2 §FS-rhei-agents.4.1
fn placed_under_artifact_root(root: &Path, path: &Path) -> Option<(PathBuf, PathBuf)> {
    if let Ok(relative) = path.strip_prefix(root) {
        return Some((relative.to_path_buf(), path.to_path_buf()));
    }
    let relative = under_artifact_root(root, path)?;
    let absolute = join_under_root(root, &relative);
    Some((relative, absolute))
}

/// `path` written the way the prompt writes an absolute path: through the
/// artifact root where it lies under one, unchanged where it does not.
// §FS-rhei-memory.1.2 §FS-rhei-agents.4.1
fn spelled_under_artifact_root(root: &Path, path: &Path) -> String {
    match placed_under_artifact_root(root, path) {
        Some((_, absolute)) => spelled_path(&absolute),
        None => spelled_path(path),
    }
}

/// The canonical spelling of a path that need not exist yet: its longest
/// existing prefix resolved, the rest appended as written. A run's transcripts
/// directory is named in a prompt before the first log is written to it; a
/// path with no existing prefix at all has no canonical spelling.
// §FS-rhei-memory.1.2
fn canonical_spelling(path: &Path) -> Option<PathBuf> {
    let mut existing = path.to_path_buf();
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    loop {
        if let Ok(canonical) = rhei_core::platform::canonical_path(&existing) {
            return Some(rest.iter().rev().fold(canonical, |acc, part| acc.join(part)));
        }
        rest.push(existing.file_name()?.to_os_string());
        if !existing.pop() {
            return None;
        }
    }
}

/// A path as an agent reads it: on Windows, canonicalization adds the `\\?\`
/// verbatim prefix, which no shell or editor wants pasted back.
fn spelled_path(path: &Path) -> String {
    let rendered = path.display().to_string();
    #[cfg(windows)]
    if let Some(plain) = rendered.strip_prefix(r"\\?\") {
        if !plain.starts_with("UNC") {
            return plain.to_string();
        }
    }
    rendered
}

/// The absolute form of a path the caller may have given relative to its own
/// cwd, which is not the reader's. §FS-rhei-memory.3.4
fn absolute_memory_path(path: &Path) -> String {
    spelled_path(&std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf()))
}

/// Keep the first `cap` lines of `body`; report whether anything was dropped.
// §FS-rhei-memory.4.5: truncation is by whole lines, never mid-line.
fn head_lines(body: &str, cap: usize) -> (String, bool) {
    let lines: Vec<&str> = body.lines().collect();
    if lines.len() <= cap {
        return (body.to_string(), false);
    }
    (lines[..cap].join("\n"), true)
}

/// Keep the last `cap` lines of `body`; report whether anything was dropped.
// §FS-rhei-memory.4.4: the newest entries are the ones worth the tokens.
fn tail_lines(body: &str, cap: usize) -> (String, bool) {
    let lines: Vec<&str> = body.lines().collect();
    if lines.len() <= cap {
        return (body.to_string(), false);
    }
    (lines[lines.len() - cap..].join("\n"), true)
}

/// Cut one line to `SUMMARY_COLUMNS` characters, marking the cut.
// §FS-rhei-memory.4.3
fn cut_to_summary_columns(line: &str) -> String {
    if line.chars().count() <= memory_caps::SUMMARY_COLUMNS {
        return line.to_string();
    }
    let kept: String = line.chars().take(memory_caps::SUMMARY_COLUMNS).collect();
    format!("{kept}\u{2026}")
}

/// Whether a line opens a `## Result` entry of a result file.
///
/// A verdict written straight into the file heads a plain `## Result`; the
/// fan-out fold re-titles each fragment as `## Result — <identity>` and
/// appends them to the same list.
///
/// Both are entries, so a matcher that knew only the plain form read a folded
/// file as one long entry and reported its oldest verdict — or its heading.
///
/// The line still has to be prose: [`last_result_entry_line`] decides that.
// §FS-rhei-memory.4.3 §FS-rhei-states.3.3
fn is_result_entry_heading(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed == "## Result" || trimmed.starts_with("## Result ")
}

/// Index of the line that opens the **last** entry of a result file, if any.
///
/// A result file quotes as often as it reports: an account that shows the
/// verdict block it is objecting to puts a `## Result — <identity>` heading
/// inside a fenced example, and read as an entry that quotation becomes the
/// standing verdict — the opposite of what the file says. So fences are
/// tracked while scanning, by the rule the plan validator applies to a plan
/// body: [`rhei_validator::code_fence_run`], which `markdown_prose` in
/// `validator_links.rs` is built from. An unclosed fence runs to the end of
/// the file, as a renderer reads it.
// §FS-rhei-memory.4.3 §FS-rhei-plan-language.3.6
fn last_result_entry_line(lines: &[&str]) -> Option<usize> {
    let mut fence: Option<(char, usize)> = None;
    let mut last = None;
    for (index, line) in lines.iter().enumerate() {
        match fence {
            Some((marker, open)) => {
                if let Some((character, run, bare)) = rhei_validator::code_fence_run(line) {
                    if character == marker && run >= open && bare {
                        fence = None;
                    }
                }
            }
            None => match rhei_validator::code_fence_run(line) {
                Some((marker, run, _)) => fence = Some((marker, run)),
                None if is_result_entry_heading(line) => last = Some(index),
                None => {}
            },
        }
    }
    last
}

/// The first non-blank line of the **last** `## Result` entry of a result file.
///
/// A result file accumulates one entry per verdict, so the last entry is the
/// standing one. A file a worker wrote by hand may carry no heading at all; its
/// first non-blank line is then the whole account's first line, which is what
/// the rule is after either way.
// §FS-rhei-memory.4.3
fn result_summary_from_body(body: &str) -> Option<String> {
    let lines: Vec<&str> = body.lines().collect();
    let start = last_result_entry_line(&lines).map(|index| index + 1).unwrap_or(0);
    lines[start..]
        .iter()
        .find(|line| !line.trim().is_empty())
        .map(|line| cut_to_summary_columns(line.trim()))
}

/// The Plan History summary of one task: a fixed slice of its result file,
/// `see above` when the prompt already pastes that file in full.
// §FS-rhei-memory.4.3
fn task_history_summary(
    render_context: &RuntimeTemplateContext<'_>,
    task_id: &TaskId,
    pasted_in_full: &BTreeSet<String>,
) -> MietteResult<String> {
    if pasted_in_full.contains(&task_id.to_string()) {
        return Ok("see above".to_string());
    }
    if let Some(path) = resolved_result_path(render_context, task_id) {
        if !prompt_source_allowed(render_context, &path) {
            return Ok("payload excluded".to_string());
        }
    }
    let Some(body) = read_task_result(render_context, task_id)? else {
        return Ok("(no result)".to_string());
    };
    Ok(result_summary_from_body(&body).unwrap_or_else(|| "(no result)".to_string()))
}

/// One rhei's transition ledger, as `(task-id, from, to)` in recorded order.
///
/// Same file `transition.previous` resolves against, read whole here because
/// the memory sections need the order, not one lookup.
// §FS-rhei-complete.3.1 §FS-rhei-memory.4.1
fn read_ledger(root: &Path) -> MietteResult<Vec<(String, String, String)>> {
    // §FS-rhei-recover.4: direct readers hold access through the read.
    let _guard = rhei_core::root_access::RootAccessGuard::shared(root).map_err(|err| miette!("{err}"))?;
    let path = root.join("runtime").join("state-transitions.log");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&path)
        .map_err(|err| file_io_report(&path, "failed to read the transition ledger", err))?;
    rhei_core::transition_history::parse(&content).map_err(|err| miette!("{err}"))?;
    Ok(parse_ledger(&content))
}

/// Parse the timestamp-free `<task-id> <from>@<to>` ledger body.
// §FS-rhei-complete.3.1
fn parse_ledger(content: &str) -> Vec<(String, String, String)> {
    // §FS-rhei-complete.3.1: one validated pair contributes one movement.
    rhei_core::transition_history::parse(content).unwrap_or_default().into_iter()
        .map(|movement| (movement.task_id, movement.from, movement.to)).collect()
}

/// Result files this prompt already pastes in full, by qualified id.
///
/// `## Prior Task Results`, `## Child Task Results`, and `## Checkpoints` are
/// composed before `## Plan History`, and detail is paid for once.
// §FS-rhei-memory.1.3 §FS-rhei-memory.4.3
fn results_pasted_in_full(
    render_context: &RuntimeTemplateContext<'_>,
) -> MietteResult<BTreeSet<String>> {
    let mut pasted = BTreeSet::new();
    let pastes_inputs =
        render_context.memory.is_some_and(|memory| memory.pastes_task_inputs);
    if pastes_inputs {
        for prior in &render_context.task.prior {
            if read_task_result(render_context, prior)?.is_some() {
                pasted.insert(prior.to_string());
            }
        }
    }
    let supervising = task_is_supervising(render_context.task, render_context.machine);
    if !supervising {
        if !pastes_inputs {
            return Ok(pasted);
        }
        for child in &render_context.task.children {
            if !task_state_is_terminal(child, render_context.machine) {
                continue;
            }
            if read_task_result(render_context, &child.id)?.is_some() {
                pasted.insert(child.id.to_string());
            }
        }
        return Ok(pasted);
    }
    for checkpoint in supervision_checkpoints(render_context.metadata, &render_context.task.id) {
        let qualified = checkpoint_qualified_id(render_context.task, &checkpoint.task);
        let Some(descendant) = checkpoint_descendant(render_context.task, &qualified) else {
            continue;
        };
        let to_is_terminal = render_context
            .machine
            .states
            .get(&checkpoint.to)
            .map(|def| def.terminal)
            .unwrap_or(false);
        if to_is_terminal && read_task_result(render_context, &descendant.id)?.is_some() {
            pasted.insert(descendant.id.to_string());
        }
    }
    Ok(pasted)
}

/// Descendants of `task` whose results this prompt already pasted, which drop
/// out of `own` rather than repeating as one-liners. §FS-rhei-memory.4.3
fn pasted_descendant_ids(
    render_context: &RuntimeTemplateContext<'_>,
    pasted_in_full: &BTreeSet<String>,
) -> BTreeSet<String> {
    flatten_task_slice(&render_context.task.children)
        .into_iter()
        .map(|task| task.id.to_string())
        .filter(|id| pasted_in_full.contains(id))
        .collect()
}
