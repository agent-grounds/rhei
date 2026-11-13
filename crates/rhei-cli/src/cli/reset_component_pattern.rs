// A declared `{task_id}` artifact path with a placeholder before the id, matched
// one path component at a time.
//
// Its own part because it is one rule with several readers: cut at its first
// placeholder, such a path leaves a prefix no ticket id stands in, and a reset that
// swept that prefix took every ticket's files under it. Both narrowings, the
// bare-id check and `rhei remove` ask it.

// §AR-source-file-size.3 §FS-rhei-reset.2.1

/// A declared artifact path that puts an unresolved placeholder (`{state}`,
/// `{rhei_id}`, …) before `{task_id}`, resolved for one ticket id. Each
/// placeholder matches any part of one path component and never a `/`, and the
/// id has to stand where `{task_id}` stands. §FS-rhei-reset.2.1
#[derive(Debug, Clone, PartialEq, Eq)]
struct ComponentPattern {
    /// The execution root the declared path is relative to.
    root: PathBuf,
    /// The declared path, `{task_id}` still in it.
    template: String,
    /// The ticket id `{task_id}` stands for.
    id: String,
    /// Each component of the resolved path as the literal pieces between its
    /// placeholders: a single piece is a literal name.
    components: Vec<Vec<String>>,
}

impl ComponentPattern {
    /// Whether `template` has another placeholder before `{task_id}`, so the
    /// literal prefix up to its first placeholder carries no ticket id.
    fn applies_to(template: &str) -> bool {
        template.contains("{task_id}") && template.find('{') != template.find("{task_id}")
    }

    fn new(root: &Path, template: &str, id: &str) -> Self {
        let components = template
            .replace("{task_id}", id)
            .split('/')
            .filter(|part| !part.is_empty() && *part != ".")
            .map(component_pieces)
            .collect();
        Self { root: root.to_path_buf(), template: template.to_string(), id: id.to_string(), components }
    }

    /// The same declared path for another ticket id.
    fn for_id(&self, id: &str) -> Self {
        Self::new(&self.root, &self.template, id)
    }

    /// Every path under `root` it matches now, read one level at a time; the
    /// directory a listing failed in, with the error, where one did.
    fn entries(&self) -> Result<Vec<PathBuf>, (PathBuf, std::io::Error)> {
        let mut level = vec![self.root.clone()];
        for (depth, pieces) in self.components.iter().enumerate() {
            let last = depth + 1 == self.components.len();
            let present = |path: &Path| {
                if last { fs::symlink_metadata(path).is_ok() } else { path.is_dir() }
            };
            let mut next = Vec::new();
            for dir in &level {
                if let [literal] = pieces.as_slice() {
                    let path = dir.join(literal);
                    if present(&path) {
                        next.push(path);
                    }
                    continue;
                }
                if !dir.is_dir() {
                    continue;
                }
                for entry in fs::read_dir(dir).map_err(|err| (dir.clone(), err))?.flatten() {
                    let path = entry.path();
                    let matched = entry.file_name().to_str().is_some_and(|name| pieces_match(pieces, name));
                    if matched && present(&path) {
                        next.push(path);
                    }
                }
            }
            level = next;
        }
        Ok(level)
    }

    /// Whether `path` is one it matches, component for component.
    fn matches(&self, path: &Path) -> bool {
        let Ok(rest) = path.strip_prefix(&self.root) else { return false };
        let names: Vec<Option<&str>> = rest.components().map(|part| part.as_os_str().to_str()).collect();
        names.len() == self.components.len()
            && names
                .iter()
                .zip(&self.components)
                .all(|(name, pieces)| name.is_some_and(|name| pieces_match(pieces, name)))
    }
}

/// One path component as the literal pieces between its placeholders:
/// `{state}-paper.2.md` is `["", "-paper.2.md"]`, `paper.2.md` is `["paper.2.md"]`.
fn component_pieces(part: &str) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut rest = part;
    while let Some(open) = rest.find('{') {
        pieces.push(rest[..open].to_string());
        rest = rest[open..].find('}').map_or("", |close| &rest[open + close + 1..]);
    }
    pieces.push(rest.to_string());
    pieces
}

/// Whether `name`, one path component, reads as `pieces` with any text, never a
/// `/`, between them.
fn pieces_match(pieces: &[String], name: &str) -> bool {
    let [first, middle @ .., last] = pieces else {
        return pieces.first().is_some_and(|literal| literal == name);
    };
    if name.len() < first.len() + last.len() || !name.starts_with(first.as_str()) || !name.ends_with(last.as_str()) {
        return false;
    }
    let mut rest = &name[first.len()..name.len() - last.len()];
    for piece in middle {
        let Some(at) = rest.find(piece.as_str()) else { return false };
        rest = &rest[at + piece.len()..];
    }
    true
}
