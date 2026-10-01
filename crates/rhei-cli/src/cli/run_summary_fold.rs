// The console task tree's subtree fold: which terminal parents speak for their
// finished subtree, what the line they print instead says, and which rows that
// leaves for the tree to print.
//
// Its own part because the fold is a rendering decision over rows the report has
// already classified, and nothing else in the report reads it: the tallies,
// `## Task Final States` and the non-TTY lines all keep one row per task. The
// clause it prints is the prompt side's, so this file decides only *when* a
// subtree folds, never how the fold is spelled.

// §AR-source-file-size.3 §FS-rhei-run-report.3.2 §FS-rhei-run-report.3.4

/// What the subtree fold decided for one row of the console tree.
enum RowFold {
    /// Printed as it always was.
    Shown,
    /// A terminal parent printed once for its whole finished subtree, carrying
    /// this detail column in place of its descendants' rows.
    Speaks(String),
    /// Under a parent that speaks for it, so the tree prints no row for it.
    Hidden,
}

/// The subtree fold of every row of the console task tree, by row index.
///
/// A task with descendants folds when it is itself terminal and **no
/// descendant is an attention row (`!`) or a gate (`⏸`)**; its descendants then
/// print no rows of their own and its detail column gains
/// ` — {n} subtasks: {breakdown}`. That clause is [`subtree_fold_clause`], the
/// one the prompt side writes, so two surfaces folding one subtree cannot spell
/// the fold two ways. An open parent, or one above a row a human must act on,
/// keeps every row. §FS-rhei-run-report.3.2 §FS-rhei-memory.3.2
#[derive(Default)]
struct SubtreeFolds(Vec<RowFold>);

impl SubtreeFolds {
    /// Decide the fold for `rows`, which [`collect_rows`] walked from `tasks`
    /// in the same plan order, parents before children.
    ///
    /// The outermost parent that folds wins: a finished parent under a
    /// finished parent is one of the rows the outer one speaks for.
    // §FS-rhei-run-report.3.2
    fn of(
        tasks: &[rhei_core::ast::Task],
        machines: &rhei_validator::MachineSet,
        rows: &[TaskRow],
    ) -> Self {
        let nodes = flatten_task_slice(tasks);
        let mut folds: Vec<RowFold> = rows.iter().map(|_| RowFold::Shown).collect();
        let mut index = 0;
        while index < rows.len() {
            let end = subtree_end(rows, index);
            // The walk and the rows are one plan order; a node that does not
            // line up with its row is never folded, so a mismatch hides nothing.
            let task = nodes.get(index).filter(|task| task.id.to_string() == rows[index].id);
            let folds_here = end > index + 1 && subtree_is_finished(&rows[index..end], machines);
            let Some(task) = task.filter(|_| folds_here) else {
                index += 1;
                continue;
            };
            let clause = subtree_fold_clause(task, machines.for_task(&task.id));
            let detail = format!("{}{clause}", rows[index].detail.as_deref().unwrap_or(""));
            folds[index] = RowFold::Speaks(detail.trim_start().to_string());
            folds[index + 1..end].iter_mut().for_each(|fold| *fold = RowFold::Hidden);
            index = end;
        }
        Self(folds)
    }

    /// The rows the tree prints, in order, each with the detail column a
    /// folded parent carries instead of its own. A row the fold never saw is
    /// printed as it always was.
    // §FS-rhei-run-report.3.2
    fn visible<'a>(
        &'a self,
        rows: &'a [TaskRow],
    ) -> impl Iterator<Item = (&'a TaskRow, Option<&'a str>)> + 'a {
        rows.iter().enumerate().filter_map(|(index, row)| match self.0.get(index) {
            Some(RowFold::Hidden) => None,
            Some(RowFold::Speaks(detail)) => Some((row, Some(detail.as_str()))),
            Some(RowFold::Shown) | None => Some((row, None)),
        })
    }
}

/// One past the last descendant of `rows[index]`: the plan-order walk puts a
/// subtree in the contiguous rows that follow its root at a greater depth.
fn subtree_end(rows: &[TaskRow], index: usize) -> usize {
    let depth = rows[index].depth;
    rows[index + 1..]
        .iter()
        .position(|row| row.depth <= depth)
        .map_or(rows.len(), |offset| index + 1 + offset)
}

/// Whether `subtree` — a parent row followed by every descendant row — is over
/// with nothing in it for a human: the parent terminal, and not one row that is
/// an attention row or a gate. Every halted, gated or otherwise open task keeps
/// its row, and so does every ancestor above it. §FS-rhei-run-report.3.2
fn subtree_is_finished(subtree: &[TaskRow], machines: &rhei_validator::MachineSet) -> bool {
    let parent = &subtree[0];
    is_terminal_state(&parent.state, machines.for_task(&parse_task_id(&parent.id)))
        && subtree.iter().all(|row| !row.marker.needs_attention())
}
