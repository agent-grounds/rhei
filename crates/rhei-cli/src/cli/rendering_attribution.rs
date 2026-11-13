// Where a refusal points when the file it is about is a template's rendering,
// answered in one place so that every diagnostic naming a location agrees.
//
// Its own part for the same reason as `help_target.rs`: the reports that name a
// rendered file are built deep inside validation, far from the lay that knows the
// file was rendered, from which template, and what became of it. The lay enters a
// scope; a diagnostic printing a location or a `rhei validate` help reads it.

// §FS-rhei-errors.4.1 §FS-rhei-errors.4

/// What became of a rendering by the time its refusal is read.
/// §FS-rhei-errors.4.1
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RenderingFate {
    /// Never published where the caller asked: `--dry-run` scratch, or a
    /// member's hidden staging.
    Proposed,
    /// Written in place by a real run, then removed with its refusal.
    Removed,
    /// Kept by `--keep-on-error`, for the reader to inspect and validate.
    Kept,
}

/// A rendered tree on this thread, and the template it was rendered from.
struct RenderingAttribution {
    /// The rendered root as laid, and as resolution spells it, since a loader can
    /// hand back either.
    roots: Vec<PathBuf>,
    template_dir: PathBuf,
    /// The template as the caller named it. §FS-rhei-errors.4.1
    template_ref: String,
    /// A built-in's directory is an extraction nobody can open. §FS-rhei-errors.4.1
    builtin: bool,
    fate: RenderingFate,
}

impl RenderingAttribution {
    fn new(
        rendered: &Path,
        template_dir: &Path,
        template_ref: &str,
        builtin: bool,
        fate: RenderingFate,
    ) -> Self {
        let mut roots = vec![rendered.to_path_buf()];
        if let Ok(resolved) = std::fs::canonicalize(rendered) {
            if resolved != rendered {
                roots.push(resolved);
            }
        }
        Self {
            roots,
            template_dir: template_dir.to_path_buf(),
            template_ref: template_ref.to_owned(),
            builtin,
            fate,
        }
    }

    /// `path` relative to the rendered root, when it lies in the rendering.
    fn relative(&self, path: &Path) -> Option<PathBuf> {
        let within = |path: &Path| {
            self.roots.iter().find_map(|root| path.strip_prefix(root).ok().map(Path::to_path_buf))
        };
        within(path).or_else(|| within(&std::fs::canonicalize(path).ok()?))
    }

    /// The template file a rendered file came from, as the reader opens it: the file
    /// at the same template-relative path, or where none is there the template by the
    /// caller's reference, never a file that does not exist. §FS-rhei-errors.4.1
    fn source(&self, relative: &Path) -> String {
        let counterpart = !relative.as_os_str().is_empty()
            && self.template_dir.join(relative).is_file();
        match (counterpart, self.builtin) {
            (true, true) => format!(
                "{} of the built-in template '{}'",
                relative.display(),
                self.template_ref
            ),
            (true, false) => display_path(&self.template_dir.join(relative)),
            (false, true) => format!("the built-in template '{}'", self.template_ref),
            (false, false) => format!("the template '{}'", self.template_ref),
        }
    }

    /// The location a diagnostic prints for the rendered `path`: the path as before,
    /// then what it is the rendering of and what became of it, in the same breath, so
    /// it never reads as a file that exists. Its lines are the rendering's, so the
    /// template file is never given a line number here. §FS-rhei-errors.4.1
    fn location(&self, path: &Path, relative: &Path) -> String {
        let fate = match self.fate {
            RenderingFate::Proposed => "proposed, not written",
            RenderingFate::Removed => "written, then removed",
            RenderingFate::Kept => "kept for inspection",
        };
        format!("{}, the rendering of {} ({fate})", display_path(path), self.source(relative))
    }
}

thread_local! {
    static RENDERING: std::cell::RefCell<Option<Arc<RenderingAttribution>>> =
        const { std::cell::RefCell::new(None) };
}

/// Put back what the thread held when a lay's validation goes out of scope, so a
/// project template's member hands the attribution back to the project lay.
struct RenderingScope {
    previous: Option<Arc<RenderingAttribution>>,
}

impl Drop for RenderingScope {
    fn drop(&mut self) {
        let _ = RENDERING.try_with(|current| current.replace(self.previous.take()));
    }
}

/// Attribute every location under the rendered root to its template for the
/// scope's lifetime. §FS-rhei-errors.4.1
#[must_use]
fn enter_rendering(attribution: RenderingAttribution) -> RenderingScope {
    let previous = RENDERING.with(|current| current.replace(Some(Arc::new(attribution))));
    RenderingScope { previous }
}

fn current_rendering() -> Option<Arc<RenderingAttribution>> {
    RENDERING.try_with(|current| current.borrow().clone()).ok().flatten()
}

/// `path` as a diagnostic names a location: as [`display_path`] spells it, or,
/// where it lies in a rendering, as the rendering of its template file.
/// §FS-rhei-errors.4.1
fn diagnostic_location(path: &Path) -> String {
    current_rendering()
        .and_then(|rendering| {
            let relative = rendering.relative(path)?;
            Some(rendering.location(path, &relative))
        })
        .unwrap_or_else(|| display_path(path))
}

/// What a help says in place of validating a rendering that is gone: fix the
/// template file or its values, and run the same instantiation again. `None` where
/// there is no rendering in scope, or it was kept and validating it stands.
/// §FS-rhei-errors.4.1
fn rendering_retry_help() -> Option<&'static str> {
    current_rendering().filter(|rendering| rendering.fate != RenderingFate::Kept).map(|_| {
        "in the template named above, or the values it was rendered with, then run \
         the same rhei instantiate again."
    })
}
