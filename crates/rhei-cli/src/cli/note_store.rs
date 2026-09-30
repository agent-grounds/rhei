// The seam the note store composes through: the fold that turns the
// append-only journal into the live entries, and the block those entries
// render as.
//
// Declared here and empty on purpose. The contract commit writes the seam so
// the cases against it fail on an assertion rather than on a missing symbol;
// the change that follows fills both bodies in and drops the `dead_code`
// allowances, which are only true while nothing calls them.

// §AR-source-file-size.3 §FS-rhei-memory.4.2 §FS-rhei-note.3

/// One live entry of the project note store: the task that wrote it, and its
/// text. A task has at most one, so the task is the identity.
// §FS-rhei-note.3.2
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
struct NoteEntry {
    task: String,
    text: String,
}

/// Fold `runtime/notes.md` into the entries a prompt composes, in **file
/// position order** — oldest first, so a renderer reverses it.
///
/// Only a task's last record counts, surviving records apply in file order, a
/// `strikes` removes the named task's entry and a `restates` moves it to the
/// restating record's position, and a record naming no live entry is inert. A
/// record that does not parse against the grammar is skipped: a prompt must
/// compose. §FS-rhei-memory.4.2
#[allow(dead_code)]
fn fold_note_store(_contents: &str) -> Vec<NoteEntry> {
    Vec::new()
}

/// `### Project Notes`, newest first, capped with the overflow line naming the
/// file that holds the rest. Empty when there is nothing live to show, so the
/// block is omitted rather than rendered blank. §FS-rhei-memory.3.1 §FS-rhei-memory.4.5
#[allow(dead_code)]
fn render_project_notes(_entries: &[NoteEntry], _store_path: &str) -> String {
    String::new()
}
