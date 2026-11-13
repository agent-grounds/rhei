// A declared `{task_id}` artifact path with a placeholder before the id, matched
// one path component at a time, and whose a path it matches is.
//
// Its own part because it is one rule with several readers: cut at its first
// placeholder, such a path leaves a prefix no ticket id stands in, and a reset that
// swept that prefix took every ticket's files under it. Both narrowings, the
// bare-id check, the writability check and `rhei remove` ask it.

// §AR-source-file-size.3 §FS-rhei-reset.2.1

/// A declared artifact path that puts an unresolved placeholder (`{state}`,
/// `{rhei_id}`, …) before `{task_id}`, resolved for one ticket id. Each
/// placeholder matches any part of one path component and never a `/`, and the
/// id has to stand where `{task_id}` stands. §FS-rhei-reset.2.1
#[derive(Debug, Clone, PartialEq, Eq)]
struct ComponentPattern {
    /// The execution root the declared path is relative to.
    root: PathBuf,
    /// The ticket id `{task_id}` stands for.
    id: String,
    components: Vec<PatternComponent>,
}

/// One path component of a [`ComponentPattern`].
#[derive(Debug, Clone, PartialEq, Eq)]
struct PatternComponent {
    /// The resolved component as the literal pieces between its placeholders: a
    /// single piece is a literal name.
    pieces: Vec<String>,
    /// Where the component holds `{task_id}`, the pieces of the declared text before
    /// its first `{task_id}` and of the resolved text after it: where the id is
    /// matched, which `pieces` no longer says.
    id_slot: Option<(Vec<String>, Vec<String>)>,
}

impl ComponentPattern {
    /// Whether `template` has another placeholder before `{task_id}`, so the
    /// literal prefix up to its first placeholder carries no ticket id.
    fn applies_to(template: &str) -> bool {
        template.contains("{task_id}") && template.find('{') != template.find("{task_id}")
    }

    fn new(root: &Path, template: &str, id: &str) -> Self {
        let components = template
            .split('/')
            .filter(|part| !part.is_empty() && *part != ".")
            .map(|part| PatternComponent {
                pieces: component_pieces(&part.replace("{task_id}", id)),
                id_slot: part.split_once("{task_id}").map(|(before, after)| {
                    (component_pieces(before), component_pieces(&after.replace("{task_id}", id)))
                }),
            })
            .collect();
        Self { root: root.to_path_buf(), id: id.to_string(), components }
    }

    /// Every path under `root` it matches now, read one level at a time; the
    /// directory a listing failed in, with the error, where one did. From its first
    /// placeholder down it enters only real directories, never a symbolic link, and a
    /// link is matched only as the last component. §FS-rhei-reset.2.1
    fn entries(&self) -> Result<Vec<PathBuf>, (PathBuf, std::io::Error)> {
        let mut level = vec![self.root.clone()];
        let mut placeholder_seen = false;
        for (depth, component) in self.components.iter().enumerate() {
            let last = depth + 1 == self.components.len();
            placeholder_seen |= component.pieces.len() > 1;
            let present = |path: &Path| match fs::symlink_metadata(path) {
                Ok(_) if last => true,
                Ok(meta) if placeholder_seen => meta.is_dir(),
                Ok(_) => path.is_dir(),
                Err(_) => false,
            };
            let mut next = Vec::new();
            for dir in &level {
                if let [literal] = component.pieces.as_slice() {
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
                    let name = entry.file_name();
                    let matched = name.to_str().is_some_and(|name| pieces_match(&component.pieces, name));
                    if matched && present(&path) {
                        next.push(path);
                    }
                }
            }
            level = next;
        }
        Ok(level)
    }

    /// Whether `path` is this ticket's: it matches, component for component, and no
    /// longer id of `ids` could stand where `{task_id}` stands in it. A placeholder
    /// reaches files of every naming, so that a longer id's own instance of the same
    /// path does not match proves nothing. §FS-rhei-reset.2.1 §FS-rhei-remove.4.2
    fn owns<'a>(&self, path: &Path, ids: impl IntoIterator<Item = &'a str>) -> bool {
        let Some(names) = self.names(path) else { return false };
        let id = self.id.as_str();
        let longer: Vec<&str> =
            ids.into_iter().filter(|other| other.len() > id.len() && other.contains(id)).collect();
        names.iter().zip(&self.components).all(|(name, component)| {
            pieces_match(&component.pieces, name)
                && component.id_slot.as_ref().is_none_or(|(before, after)| {
                    !longer_id_stands(name, id, before, after, &longer)
                })
        })
    }

    /// `path`'s components under `root`, where there are as many as it has.
    fn names<'p>(&self, path: &'p Path) -> Option<Vec<&'p str>> {
        let rest = path.strip_prefix(&self.root).ok()?;
        let names: Option<Vec<&str>> = rest.components().map(|part| part.as_os_str().to_str()).collect();
        names.filter(|names| names.len() == self.components.len())
    }
}

/// Whether one of `longer` could stand where `{task_id}` stands in `name`: it
/// begins where the text before it reads as `before`, the declared text before
/// `{task_id}`, and runs over a place `id` is matched at. Nothing after the id is
/// asked of it. §FS-rhei-reset.2.1
fn longer_id_stands(name: &str, id: &str, before: &[String], after: &[String], longer: &[&str]) -> bool {
    let starts: Vec<usize> = (0..=name.len())
        .filter(|&at| name.is_char_boundary(at) && pieces_match(before, &name[..at]))
        .collect();
    let mine: Vec<usize> = starts
        .iter()
        .copied()
        .filter(|&at| name[at..].starts_with(id) && pieces_match(after, &name[at + id.len()..]))
        .collect();
    longer.iter().any(|other| {
        starts.iter().any(|&from| {
            name[from..].starts_with(other)
                && mine.iter().any(|&at| from <= at && at + id.len() <= from + other.len())
        })
    })
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
