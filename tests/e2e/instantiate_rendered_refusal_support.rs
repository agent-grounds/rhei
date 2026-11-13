//! The fixture and the reading for §FS-rhei-errors.4.1: a template whose
//! rendering does not parse, discovered from a repository's own template home,
//! laid standalone or as a member of a Panta project beside that repository, and
//! a refusal read as the paragraphs a person sees.
//!
//! The reading is deliberately loose about wording, which is the implementer's.
//! It holds a refusal to three things only: the template file it names exists
//! where it says, a rendered location is marked as a rendering with its status
//! in the paragraph that shows it, and the parse reason is still there.

use std::fs;
use std::path::{Path, PathBuf};

use super::*;

/// Where a repository keeps its own templates, from the repository root.
pub const TEMPLATE_HOME: &str = ".agent-grounds/rhei/templates";

/// The issue's `index.rhei.md`: a plan that still carries the retired line.
pub const STATES_LINE_INDEX: &str = "# Rhei: Forge metadata\n\n**States:** forge-metadata\n";

/// The issue's task fragment, which parses.
pub const TASK: &str = "### Task one: Do the thing\n**State:** todo\n";

/// A task fragment that does not parse: the retired line as task metadata.
pub const STATES_LINE_TASK: &str =
    "### Task one: Do the thing\n**State:** todo\n**States:** forge-metadata\n";

/// What the parse says about the retired line in a plan, and in a fragment.
pub const INDEX_REASON: &str = "`**States:**` is no longer part of the plan language";
pub const TASK_REASON: &str = "Unknown metadata field '**States:**'";

/// Words that say a location is not a file the reader can open.
const STATUSES: &[&str] = &["not written", "proposed", "removed", "discarded"];

/// The issue's machine, named so an `includes:` entry can read it as well.
pub fn machine(name: &str) -> String {
    format!(
        "name: {name}\nversion: 1\ninitial: todo\nstates:\n  todo:\n    description: Not started.\n  \
         done:\n    description: Finished.\n    final: true\ntransitions:\n  - {{from: todo, to: done}}\n"
    )
}

/// A repository to instantiate from, and, for a member, the Panta project beside
/// it that the default output lands in. Scratch is a directory of its own, so a
/// refusal can be held to never naming it.
pub struct Layout {
    _root: TestDir,
    pub repo: PathBuf,
    pub panta: PathBuf,
    home: PathBuf,
    pub scratch: PathBuf,
}

impl Layout {
    /// No Panta project above the repository: the default output is
    /// `./<template>`, rendered in place.
    pub fn standalone(prefix: &str) -> Self {
        let root = unique_temp_dir(prefix);
        let layout = Self {
            repo: root.join("repo"),
            panta: root.join("panta"),
            home: root.join("home"),
            scratch: root.join("scratch"),
            _root: root,
        };
        for dir in [&layout.repo, &layout.home, &layout.scratch] {
            fs::create_dir_all(dir).expect("create the layout's directories");
        }
        layout
    }

    /// The issue's layout: `rhei init` lays a Panta project beside the
    /// repository, so the default output is `../panta/<template>`, staged.
    pub fn member(prefix: &str) -> Self {
        let layout = Self::standalone(prefix);
        let panta = layout.panta.to_str().expect("a UTF-8 fixture path");
        let init = layout.rhei(&["init", "--here", "--no-agents", panta]);
        assert!(init.status.success(), "rhei init should lay the project:\n{}", init.stderr);
        layout
    }

    /// A template under the repository's template home, by file name and text.
    pub fn template(&self, name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = self.repo.join(TEMPLATE_HOME).join(name);
        for (file, text) in files {
            let path = dir.join(file);
            fs::create_dir_all(path.parent().expect("a template file has a parent"))
                .expect("create the template's directories");
            fs::write(&path, text).expect("write the template file");
        }
        dir
    }

    /// The issue's `forge-metadata` template, with its plan and its one task.
    pub fn forge_metadata(&self, index: &str, task: &str) -> PathBuf {
        let manifest =
            "name: forge-metadata\nversion: 1.0.0\ndescription: A plan that still carries a States line\n";
        let machine = machine("forge-metadata");
        self.template(
            "forge-metadata",
            &[
                ("template.yaml", manifest),
                ("states.yaml", &machine),
                ("index.rhei.md", index),
                ("tasks/01-task.md", task),
            ],
        )
    }

    /// `rhei <args>` typed in the repository, with scratch kept to this layout.
    pub fn rhei(&self, args: &[&str]) -> CliRun {
        let out = rhei_command(&self.home)
            .current_dir(&self.repo)
            .env("TMPDIR", &self.scratch)
            .env("TMP", &self.scratch)
            .env("TEMP", &self.scratch)
            .args(args)
            .output()
            .expect("run rhei");
        CliRun::from(&out)
    }

    /// `rhei instantiate <args>` refused for the reason the fixture plants, with
    /// nothing left behind in scratch.
    pub fn refused(&self, args: &[&str], reason: &str) -> String {
        let mut full = vec!["instantiate"];
        full.extend_from_slice(args);
        let out = self.rhei(&full);
        let text = out.stderr;
        assert_eq!(out.status.code(), Some(1), "the rendering should be refused:\n{text}");
        assert!(text.contains("PARSE ERROR"), "the fixture should reach the parse:\n{text}");
        assert!(text.contains(reason), "the parse reason stays whole:\n{text}");
        let left: Vec<_> = fs::read_dir(&self.scratch).expect("read scratch").collect();
        assert!(left.is_empty(), "scratch is discarded on refusal: {left:?}");
        let scratch = self.scratch.display().to_string();
        assert!(!text.contains(&scratch), "a refusal never names scratch:\n{text}");
        text
    }
}

/// The paragraphs of a soft-wrap-undone refusal, gutter stripped. A blank line
/// ends one, and so does the start of the `help:` footer.
pub fn paragraphs(text: &str) -> Vec<String> {
    let mut paragraphs = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for line in text.lines() {
        let line = line.trim_start();
        let help = line.starts_with("help:");
        let body = ["\u{d7}", "\u{2502}", "help:", "\u{26a0}", "\u{261e}"]
            .iter()
            .find_map(|gutter| line.strip_prefix(gutter))
            .unwrap_or(line)
            .trim();
        if (body.is_empty() || help) && !current.is_empty() {
            paragraphs.push(current.join("\n"));
            current.clear();
        }
        if !body.is_empty() {
            current.push(body);
        }
    }
    if !current.is_empty() {
        paragraphs.push(current.join("\n"));
    }
    paragraphs
}

/// The refusal names `file` of template `name`, as the reader opens it from the
/// repository: the path through the template home, relative or absolute.
/// §FS-rhei-errors.4.1
pub fn assert_names_template_file(layout: &Layout, text: &str, name: &str, file: &str) {
    let shown = format!("{TEMPLATE_HOME}/{name}/{file}");
    assert!(layout.repo.join(&shown).is_file(), "the fixture should have written {shown}");
    assert!(text.contains(&shown), "the refusal should name the template file {shown}:\n{text}");
}

/// Every path the refusal names through the template home is a file there: the
/// template file it names is the one it came from, never a guess.
/// §FS-rhei-errors.4.1
pub fn assert_template_paths_exist(layout: &Layout, text: &str) {
    for path in paths_through(text, TEMPLATE_HOME) {
        let opened = if Path::new(&path).is_absolute() {
            PathBuf::from(&path)
        } else {
            layout.repo.join(&path)
        };
        assert!(opened.is_file(), "the refusal names {path}, which is no template file:\n{text}");
    }
}

/// Every path-shaped token in `text` that runs through `segment`.
fn paths_through(text: &str, segment: &str) -> Vec<String> {
    let opens = |c: char| c.is_whitespace() || "'\"`(<[".contains(c);
    let continues = |c: char| c.is_alphanumeric() || "-_./".contains(c);
    let mut paths = Vec::new();
    for (at, _) in text.match_indices(segment) {
        let start = text[..at].rfind(opens).map_or(0, |open| open + 1);
        let end = text[at..].find(|c: char| !continues(c)).map_or(text.len(), |stop| at + stop);
        paths.push(text[start..end].trim_end_matches('.').to_owned());
    }
    paths
}

/// Each paragraph showing the rendered location `rendered` — any occurrence not
/// reached through the template home — says it is a rendering and gives its
/// status: proposed or not written, or removed. §FS-rhei-errors.4.1
pub fn assert_rendered_marked(text: &str, rendered: &str) {
    let home = format!("{TEMPLATE_HOME}/");
    for paragraph in paragraphs(text) {
        let shown = paragraph.match_indices(rendered).any(|(at, _)| {
            let before = &paragraph[..at];
            !before.ends_with(&home) && !before.ends_with("templates/")
        });
        if !shown {
            continue;
        }
        let lower = paragraph.to_lowercase();
        assert!(
            lower.contains("render") && STATUSES.iter().any(|status| lower.contains(status)),
            "the rendered location {rendered} is shown without saying it is a rendering that \
             was not written, or was removed:\n{paragraph}\n\nwhole refusal:\n{text}"
        );
    }
}

/// The refusal offers no `rhei validate`: nothing is left where the rendering
/// was. §FS-rhei-errors.4.1 §FS-rhei-library.7.5
pub fn assert_offers_no_validate(text: &str) {
    assert!(!text.contains("rhei validate"), "nothing is left to validate:\n{text}");
}
