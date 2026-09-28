// `rhei show` — one ticket's heading and its body, by id, and nothing else.
//
// Its own part because reading is the whole command: it resolves no state
// machine, takes no lock, writes no file, and asks nothing about readiness, so
// a finished ticket reads exactly the way a draft one does. Everything it needs
// already exists — the shared ticket-target split, the shared qualified-or-bare
// resolver, and the plan loader — so what is here is the printing.

// §FS-rhei-show

/// The machine form of one ticket: its qualified id, its title, and its body.
///
/// Exactly three fields, in the order `--json` prints them. `rhei list --json`
/// owns a ticket's state, kind, assignee, prior, parent and depth, and two
/// surfaces that must agree about a ticket's state is one too many.
// §FS-rhei-show.4
#[derive(serde::Serialize)]
struct ShownTask<'a> {
    id: String,
    title: &'a str,
    content: &'a str,
}

/// Execute the `show` subcommand: print one task's heading and body by id.
///
/// The ticket target is split out of the positional by
/// [`split_show_ticket_target`] before this runs, so an id-shaped argument never
/// reaches the plan-only resolver that would call it a bad path.
// §FS-rhei-show.1 §FS-rhei-show.3 §FS-rhei-show.5
fn show_command(
    input: &Path,
    rhei_scope: &[String],
    task_id_str: &str,
    as_json: bool,
) -> MietteResult<()> {
    let input_buf = normalize_workspace_input(input);
    let input = input_buf.as_path();
    // Reading one ticket is what an author reaches for *while* a plan is broken
    // elsewhere, so a rhei that fails to load is reported and the rest stays
    // readable — `rhei list`'s bargain, for the same reason. §FS-rhei-show
    let loaded = load_plan_leniently(input)?;
    for skipped in &loaded.unloadable {
        eprintln!("warning: {skipped}");
    }
    // No `--rhei` on this command: the explicit ticket target is the scope,
    // narrowed by the rhei the invocation was pointed at. §FS-rhei-show.2
    let scope = resolve_rhei_scope(&loaded, rhei_scope)?;
    let task_id = resolve_cli_task_id(&loaded, task_id_str, &scope)?;
    // Any ticket, in any state: readiness and terminality are promises about
    // claiming, and neither is a property of the prose. §FS-rhei-show.2
    let task = find_task_by_id_str(&loaded.rhei.tasks, &task_id).ok_or_else(|| {
        miette!(help = task_id_help(), "task '{}' not found in the plan", task_id)
    })?;
    // Surrounding blank lines are not content; the interior is untouched, an
    // appended lifecycle record included. §FS-rhei-show.3
    let body = task.content.trim();
    if as_json {
        // The text form's body minus its closing newline, and `""` rather than
        // absent for an empty one, so a script reads three keys every time.
        // §FS-rhei-show.4
        let payload = ShownTask { id: task.id.to_string(), title: &task.title, content: body };
        let rendered = serde_json::to_string_pretty(&payload)
            .map_err(|err| diagnostic!("failed to render task '{}' as JSON: {err}", task_id))?;
        println!("{rendered}");
        return Ok(());
    }
    // The heading spelling `rhei next --peek` and the run prompt already use, so
    // one ticket is not named two ways across two screens. §FS-rhei-show.3
    println!("## Task {}: {}", task.id, task.title);
    if !body.is_empty() {
        println!();
        println!("{body}");
    }
    Ok(())
}
