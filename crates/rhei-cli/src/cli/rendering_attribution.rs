// Where a refusal points when the file it is about is a template's rendering,
// answered in one place so that every diagnostic naming a location agrees.
//
// Its own part for the same reason as `help_target.rs`: the reports that name a
// rendered file are built deep inside validation, far from the lay that knows the
// file was rendered, from which template, and what became of it. The lay enters a
// scope; a diagnostic printing a location, a machine or a re-check reads it.

// §FS-rhei-errors.4.1 §FS-rhei-errors.4

/// What became of a rendering by the time its refusal is read, which is what the
/// failing phase does on disk, never what the flags asked for. §FS-rhei-errors.4.1
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RenderingFate {
    /// Never published where the caller asked: `--dry-run` scratch, a hidden
    /// staging directory, or a placement refused before it wrote.
    Proposed,
    /// Written in place by a real run, then removed with its refusal.
    Removed,
    /// Kept by `--keep-on-error`, for the reader to inspect and validate.
    Kept,
}

impl RenderingFate {
    fn words(self) -> &'static str {
        match self {
            Self::Proposed => "proposed, not written",
            Self::Removed => "written, then removed",
            Self::Kept => "kept for inspection",
        }
    }
}

/// Which files under a rendered root are the rendering, and what each came from.
#[derive(Clone, Debug)]
enum RenderedFiles {
    /// Every file under the root, from the file at its own path in the template.
    Tree,
    /// Only these root-relative files, each from the template-relative file
    /// beside it: what a placement adds to a target whose own files stay
    /// themselves. §FS-rhei-library.7.5
    Listed(Vec<(PathBuf, PathBuf)>),
}

/// A rendered tree on this thread, and the template it was rendered from.
#[derive(Clone, Debug)]
struct RenderingAttribution {
    /// The rendered root as laid. A loader may hand back its resolved spelling,
    /// so that is read too, when a location is asked for.
    root: PathBuf,
    files: RenderedFiles,
    template_dir: PathBuf,
    /// The template as the caller named it. §FS-rhei-errors.4.1
    template_ref: String,
    /// A built-in's directory is an extraction nobody can open. §FS-rhei-errors.4.1
    builtin: bool,
    /// Whether the template has `includes:`, whose files an unmatched rendering
    /// may have come from. §FS-rhei-library.6
    includes: bool,
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
        Self {
            root: rendered.to_path_buf(),
            files: RenderedFiles::Tree,
            template_dir: template_dir.to_path_buf(),
            template_ref: template_ref.to_owned(),
            builtin,
            includes: false,
            fate,
        }
    }

    fn including(self, includes: bool) -> Self {
        Self { includes, ..self }
    }

    /// The same rendering placed under `root` as only `files`, each beside the
    /// template-relative file it came from. §FS-rhei-library.7.5
    fn placed(self, root: &Path, files: Vec<(PathBuf, PathBuf)>) -> Self {
        Self { root: root.to_path_buf(), files: RenderedFiles::Listed(files), ..self }
    }

    fn with_fate(self, fate: RenderingFate) -> Self {
        Self { fate, ..self }
    }

    /// `path` relative to the rendered root, when it lies in it.
    fn relative(&self, path: &Path) -> Option<PathBuf> {
        let within = |path: &Path, root: &Path| path.strip_prefix(root).ok().map(Path::to_path_buf);
        within(path, &self.root).or_else(|| {
            let root = std::fs::canonicalize(&self.root).ok()?;
            within(path, &root).or_else(|| within(&std::fs::canonicalize(path).ok()?, &root))
        })
    }

    /// The template-relative file a rendered `path` came from, when the path is
    /// part of this rendering.
    fn source_of(&self, path: &Path) -> Option<PathBuf> {
        let relative = self.relative(path)?;
        match &self.files {
            RenderedFiles::Tree => Some(relative),
            RenderedFiles::Listed(files) => files
                .iter()
                .find(|(rendered, _)| *rendered == relative)
                .map(|(_, source)| source.clone()),
        }
    }

    /// The template file a rendered file came from, as the reader opens it: the
    /// recorded file, or where the template has none there the template by the
    /// caller's reference, never a file that does not exist. §FS-rhei-errors.4.1
    fn source(&self, relative: &Path) -> String {
        // The one file materialize moves: its root settings. §FS-rhei-templates.6.1.2
        let settings = relative == rendered_template_settings(Path::new(""));
        let relative = if settings { Path::new(TEMPLATE_SETTINGS_FILE) } else { relative };
        let counterpart = !relative.as_os_str().is_empty()
            && self.template_dir.join(relative).is_file();
        let named = match (counterpart, self.builtin) {
            (true, true) => {
                format!("{} of the built-in template '{}'", relative.display(), self.template_ref)
            }
            (true, false) => display_path(&self.template_dir.join(relative)),
            (false, true) => format!("the built-in template '{}'", self.template_ref),
            (false, false) => format!("the template '{}'", self.template_ref),
        };
        // An `includes:` union renames what it joins and joins its templates'
        // settings, so the line may be in one of those templates instead. §FS-rhei-errors.4.1
        if self.includes && (!counterpart || settings) {
            format!("{named} or a template it includes")
        } else {
            named
        }
    }
}

/// A rendered file as a refusal names it.
struct RenderedOrigin {
    /// `the rendering of <template file> (<fate>)`, said beside the rendered path
    /// so it never reads as a file that exists. Its lines are the rendering's, so
    /// the template file is never given a line number here.
    rendering: String,
    /// Whether the rendering is gone or never written, so nothing re-checks it.
    discarded: bool,
}

thread_local! {
    static RENDERINGS: std::cell::RefCell<Vec<RenderingAttribution>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Hand the thread back what it held when a phase's scope goes, so a project
/// template's member returns the attribution to the project lay.
struct RenderingScope {
    depth: usize,
}

impl Drop for RenderingScope {
    fn drop(&mut self) {
        let _ = RENDERINGS.try_with(|stack| stack.borrow_mut().truncate(self.depth));
    }
}

/// Attribute the rendered files to their template for the scope's lifetime.
/// Scopes nest: a path the innermost does not hold falls back to the enclosing
/// ones, so a member's validation still names its project's own files.
/// §FS-rhei-errors.4.1
#[must_use]
fn enter_rendering(attribution: RenderingAttribution) -> RenderingScope {
    let depth = RENDERINGS.with(|stack| {
        let mut stack = stack.borrow_mut();
        stack.push(attribution);
        stack.len() - 1
    });
    RenderingScope { depth }
}

/// The innermost rendering in scope that holds `path`. §FS-rhei-errors.4.1
fn rendered_origin(path: &Path) -> Option<RenderedOrigin> {
    RENDERINGS
        .try_with(|stack| {
            stack.borrow().iter().rev().find_map(|rendering| {
                let source = rendering.source_of(path)?;
                Some(RenderedOrigin {
                    rendering: format!(
                        "the rendering of {} ({})",
                        rendering.source(&source),
                        rendering.fate.words()
                    ),
                    discarded: rendering.fate != RenderingFate::Kept,
                })
            })
        })
        .ok()
        .flatten()
}

/// `path` as a diagnostic names a location: as [`display_path`] spells it, then,
/// where it lies in a rendering, whose rendering it is. §FS-rhei-errors.4.1
fn diagnostic_location(path: &Path) -> String {
    match rendered_origin(path) {
        Some(origin) => format!("{}, {}", display_path(path), origin.rendering),
        None => display_path(path),
    }
}

/// Whose rendering a machine a validation names is, when it is one: the
/// target's own machines are themselves. §FS-rhei-errors.4.1 §FS-rhei-library.7.5
fn machine_rendering(path: Option<&Path>) -> Option<String> {
    path.and_then(rendered_origin).map(|origin| origin.rendering)
}

/// The settings file a template may ship at its root. §FS-rhei-templates.6.1.2
const TEMPLATE_SETTINGS_FILE: &str = "settings.json";

/// Where a template's root `settings.json` is rendered under `rendered`: rhei's
/// project-local home, not beside the plan. Materialize writes it here and a
/// refusal reads it back as that file, so both take it from this one rule.
/// Nothing else is rendered there, because materialize never copies a template's
/// dot-directories. §FS-rhei-templates.6.1.2 §FS-rhei-errors.4.1
fn rendered_template_settings(rendered: &Path) -> PathBuf {
    rhei_home_write_path(rendered, TEMPLATE_SETTINGS_FILE)
}

/// A file as a refusal quotes it, `'<path>'`, then, where it lies in a rendering,
/// whose rendering it is; and, when that rendering is gone, the retry a help gives
/// in place of re-checking it. A file in no rendering is quoted as it always was.
/// §FS-rhei-errors.4.1
fn quoted_rendered_file(path: &Path) -> (String, Option<&'static str>) {
    let quoted = format!("'{}'", path.display());
    match rendered_origin(path) {
        Some(origin) => (
            format!("{quoted}, {}", origin.rendering),
            origin.discarded.then_some(RENDERING_RETRY),
        ),
        None => (quoted, None),
    }
}

/// The same instantiation again, after fixing what the template rendered.
const RENDERING_RETRY: &str =
    "in the template named above, or the values it was rendered with, then run the same rhei \
     instantiate again.";

/// What a help says in place of validating a rendering that is gone: fix the
/// template file or its values, and run the same instantiation again. `None` where
/// there is no rendering in scope, or the innermost was kept and validating it
/// stands. §FS-rhei-errors.4.1
fn rendering_retry_help() -> Option<&'static str> {
    let discarded = RENDERINGS
        .try_with(|stack| stack.borrow().last().map(|rendering| rendering.fate))
        .ok()
        .flatten()
        .is_some_and(|fate| fate != RenderingFate::Kept);
    discarded.then_some(RENDERING_RETRY)
}
