// The seam the note store composes through: the record grammar the
// append-only journal is written in, the fold that turns it into the live
// entries, and the block those entries render as.
//
// Its own part because reading the store is composition's business and writing
// it is the verb's, while the grammar between them belongs to neither: apart
// from where the file lives, everything here is a pure function of bytes, so
// the fold and the block are read against the memory spec with no filesystem.

// §AR-source-file-size.3 §FS-rhei-memory.4.2 §FS-rhei-note.3

/// Where the store lives for a plan input: `runtime/notes.md` at the **project**
/// execution root, which for a bare rhei is the rhei's own root — the same
/// implicit Panta every other memory section resolves against.
// §FS-rhei-note.3.1 §AR-rhei-panta.5
fn project_note_store_path(input: &Path) -> PathBuf {
    execution_workspace_root(input).join("runtime").join("notes.md")
}

/// One live entry of the project note store: the task that wrote it, and its
/// text. A task has at most one, so the task is the identity.
// §FS-rhei-note.3.2
#[derive(Clone, Debug, PartialEq, Eq)]
struct NoteEntry {
    task: String,
    text: String,
}

/// One record as the store's closed grammar spells it: a task leaving a fact of
/// its own, or spending the same slot on somebody else's entry.
///
/// Three forms and no fourth, which is what keeps a note whose own text begins
/// with the word `strikes` from being read as a verb. §FS-rhei-note.3.2
#[derive(Clone, Debug, PartialEq, Eq)]
enum NoteRecord {
    /// `- [<task-id>] <text>`
    Left { task: String, text: String },
    /// `- [<task-id> restates <task-id>]`
    Restates { task: String, target: String },
    /// `- [<task-id> strikes <task-id>]`
    Strikes { task: String, target: String },
}

impl NoteRecord {
    /// The task whose slot this record spends.
    fn writer(&self) -> &str {
        match self {
            NoteRecord::Left { task, .. }
            | NoteRecord::Restates { task, .. }
            | NoteRecord::Strikes { task, .. } => task,
        }
    }

    /// The record as one markdown list item, continuation lines indented so a
    /// three-line fact stays one item. §FS-rhei-note.3.2
    fn to_markdown(&self) -> String {
        match self {
            NoteRecord::Left { task, text } => {
                let mut out = String::new();
                for (index, line) in text.lines().enumerate() {
                    if index == 0 {
                        out.push_str(&format!("- [{task}] {line}\n"));
                    } else {
                        out.push_str(&format!("  {line}\n"));
                    }
                }
                out
            }
            NoteRecord::Restates { task, target } => format!("- [{task} restates {target}]\n"),
            NoteRecord::Strikes { task, target } => format!("- [{task} strikes {target}]\n"),
        }
    }
}

/// Whether `raw` can name a task inside the bracket: something, and one token.
///
/// Deliberately not the CLI's id shape — the fold answers for bytes already in
/// the file, and a store written by a newer rhei must not become unparsable to
/// an older one over a segment spelling. §FS-rhei-note.3.2
fn note_id_shaped(raw: &str) -> bool {
    !raw.is_empty() && !raw.contains(char::is_whitespace)
}

/// The list items of the store, each as its opening line and its indented
/// continuation lines.
///
/// A line that is neither a list item nor indented under the open one closes
/// it and contributes nothing: the store is markdown, and a stray line in it is
/// not a record. §FS-rhei-note.3.2
fn note_store_items(contents: &str) -> Vec<String> {
    let mut items: Vec<String> = Vec::new();
    let mut open = false;
    for line in contents.lines() {
        if let Some(rest) = line.strip_prefix("- ") {
            items.push(rest.to_string());
            open = true;
            continue;
        }
        // Indented, so markdown reads it as a continuation of the open item —
        // including an indented blank line, which is how an entry with a blank
        // line inside it survives the round trip.
        let indented = line.starts_with(' ') || line.starts_with('\t');
        if open && indented {
            let dedented = line.strip_prefix("  ").unwrap_or_else(|| line.trim_start());
            let item = items.last_mut().expect("an open item has a last element");
            item.push('\n');
            item.push_str(dedented);
            continue;
        }
        open = false;
    }
    items
}

/// One list item read against the grammar, or `None` when it does not parse.
///
/// The bracket is read only at the start of the item, and only these three
/// shapes are records. §FS-rhei-note.3.2
fn parse_note_item(item: &str) -> Option<NoteRecord> {
    let inside = item.strip_prefix('[')?;
    let close = inside.find(']')?;
    let (bracket, rest) = (&inside[..close], &inside[close + 1..]);
    let mut words = bracket.split_whitespace();
    let task = words.next()?.to_string();
    if !note_id_shaped(&task) {
        return None;
    }
    match (words.next(), words.next(), words.next()) {
        (None, _, _) => {
            // A fact of the writer's own. Empty text is no fact, so the item
            // does not parse rather than composing a blank line.
            if bracket.trim() != task {
                return None;
            }
            let text = rest.strip_prefix(' ').unwrap_or(rest);
            let text = text.trim_end().to_string();
            if text.trim().is_empty() { None } else { Some(NoteRecord::Left { task, text }) }
        }
        (Some(verb), Some(target), None) if rest.trim().is_empty() && note_id_shaped(target) => {
            let target = target.to_string();
            match verb {
                "restates" => Some(NoteRecord::Restates { task, target }),
                "strikes" => Some(NoteRecord::Strikes { task, target }),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Every record of the store, in file order, unparsable items skipped.
// §FS-rhei-memory.4.2 §FS-rhei-note.3.2
fn parse_note_store(contents: &str) -> Vec<NoteRecord> {
    note_store_items(contents).iter().filter_map(|item| parse_note_item(item)).collect()
}

/// Fold `runtime/notes.md` into the entries a prompt composes, in **file
/// position order** — oldest first, so a renderer reverses it.
///
/// Only a task's last record counts, surviving records apply in file order, a
/// `strikes` removes the named task's entry and a `restates` moves it to the
/// restating record's position, and a record naming no live entry is inert. A
/// record that does not parse against the grammar is skipped: a prompt must
/// compose. §FS-rhei-memory.4.2
fn fold_note_store(contents: &str) -> Vec<NoteEntry> {
    let records = parse_note_store(contents);
    // A task's last record is the only one that counts, so the earlier ones are
    // superseded here rather than by rewriting the file. §FS-rhei-note.3.4
    let mut last_by_writer: HashMap<&str, usize> = HashMap::new();
    for (index, record) in records.iter().enumerate() {
        last_by_writer.insert(record.writer(), index);
    }
    let mut live: Vec<NoteEntry> = Vec::new();
    for (index, record) in records.iter().enumerate() {
        if last_by_writer.get(record.writer()) != Some(&index) {
            continue;
        }
        match record {
            NoteRecord::Left { task, text } => {
                live.push(NoteEntry { task: task.clone(), text: text.clone() });
            }
            NoteRecord::Strikes { target, .. } => {
                live.retain(|entry| &entry.task != target);
            }
            NoteRecord::Restates { target, .. } => {
                // To this record's own position, which is what makes the
                // restated entry the newest. §FS-rhei-memory.4.2
                if let Some(at) = live.iter().position(|entry| &entry.task == target) {
                    let entry = live.remove(at);
                    live.push(entry);
                }
            }
        }
    }
    live
}

/// One entry as the block prints it, continuation lines indented under it.
// §FS-rhei-memory.3.1
fn render_note_entry(entry: &NoteEntry) -> String {
    NoteRecord::Left { task: entry.task.clone(), text: entry.text.clone() }.to_markdown()
}

/// How many lines of the block one entry costs. §FS-rhei-memory.4.5
fn note_entry_lines(entry: &NoteEntry) -> usize {
    entry.text.lines().count().max(1)
}

/// `### Project Notes`, newest first, capped with the overflow line naming the
/// file that holds the rest. Empty when there is nothing live to show, so the
/// block is omitted rather than rendered blank. §FS-rhei-memory.3.1 §FS-rhei-memory.4.5
fn render_project_notes(entries: &[NoteEntry], store_path: &str) -> String {
    if entries.is_empty() {
        return String::new();
    }
    // Newest first is descending file position, and the cap is on lines rather
    // than entries, so an entry is taken whole or not at all.
    let mut kept: Vec<&NoteEntry> = Vec::new();
    let mut lines = 0usize;
    for entry in entries.iter().rev() {
        let cost = note_entry_lines(entry);
        if lines + cost > memory_caps::PROJECT_NOTES_LINES {
            break;
        }
        lines += cost;
        kept.push(entry);
    }
    let mut out = String::from(
        "\n### Project Notes\n\nFacts earlier tickets left for whoever came next, newest \
         first.\n\n",
    );
    let dropped = entries.len() - kept.len();
    if dropped > 0 {
        out.push_str(&format!(
            "\u{2026} {dropped} earlier notes not shown \u{2014} read {store_path}\n"
        ));
    }
    for entry in kept {
        out.push_str(&render_note_entry(entry));
    }
    out
}
