// Which block a file's text has, as the load that read it decides: the one
// answer every writer that rereads a loaded file takes its fence reader from.
//
// Its own part because it is a fact about one load's record, beside the loader
// rather than in it.

// §AR-source-file-size.3 §FS-rhei-plan-language.2.1

use rhei_core::header::DocumentRole;

impl LoadedPlan {
    /// The role `file` was read in by this load: a file tickets were read from
    /// that is not a rhei's own plan document is a task file under a workspace's
    /// `tasks/` or the project's `basin/`, and any other file, a plan given
    /// directly included, is a plan. The load decides, never the path, so a plan
    /// that sits in a `tasks/` directory and a task file named by any spelling
    /// are each read the way the loader read them.
    /// §FS-rhei-plan-language.2.1 §FS-rhei-plan-language.1.4
    fn document_role(&self, file: &Path) -> DocumentRole {
        // `Some(plan document?)` for a file tickets were read from, `None` otherwise.
        let read_as = |same: &dyn Fn(&Path) -> bool| {
            self.task_sources
                .values()
                .any(|source| same(source))
                .then(|| self.rhei_plans.values().any(|plan| same(plan)))
        };
        let plan_document = read_as(&|path| path == file).or_else(|| {
            // The same file spelled another way, as a reload's error may name it.
            let file = fs::canonicalize(file).ok()?;
            read_as(&|path| fs::canonicalize(path).is_ok_and(|path| path == file))
        });
        match plan_document {
            Some(false) => DocumentRole::TaskFile,
            Some(true) | None => DocumentRole::Plan,
        }
    }
}
