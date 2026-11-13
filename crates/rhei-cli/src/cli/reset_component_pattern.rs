// A declared `{task_id}` artifact path with another placeholder, matched in full
// one path component at a time, and the one rule that says whose a path it
// matches is.
//
// Its own part because it is one rule with several readers: cut at its first
// placeholder, such a path either carries no ticket id or drops the text after the
// last placeholder, and a reset that swept that prefix took other tickets' files.
// Both narrowings, the bare-id check, the writability check and `rhei remove` ask
// it, and so do the prefixes rhei writes itself.

// §AR-source-file-size.3 §FS-rhei-reset.2.1

/// A declared artifact path that still carries an unresolved placeholder (`{state}`,
/// `{visit_count}`, `{rhei_id}`, …) once `{task_id}` is resolved for one ticket id,
/// before the id or after it. Each placeholder matches any part of one path component
/// and never a `/`, the id has to stand where `{task_id}` stands, and the literal text
/// around every placeholder, the text after the last one included, has to be there.
/// §FS-rhei-reset.2.1
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
    /// Whether `template` has another placeholder besides `{task_id}`, so it names no
    /// one file and is matched one component at a time. §FS-rhei-reset.2.1
    fn applies_to(template: &str) -> bool {
        template.contains("{task_id}") && template.replace("{task_id}", "").contains('{')
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
    /// placeholder, or the component holding the id where that comes first, it enters
    /// only real directories, never a symbolic link. A link at or below the id's
    /// component is one of the paths, never entered. §FS-rhei-reset.2.1
    fn entries(&self) -> Result<Vec<PathBuf>, (PathBuf, std::io::Error)> {
        let mut level = vec![self.root.clone()];
        let mut links = Vec::new();
        let (mut guarded, mut at_id) = (false, false);
        for (depth, component) in self.components.iter().enumerate() {
            let last = depth + 1 == self.components.len();
            at_id |= component.id_slot.is_some();
            guarded |= at_id || component.pieces.len() > 1;
            let mut next = Vec::new();
            let mut take = |path: PathBuf| match fs::symlink_metadata(&path) {
                Ok(_) if last => next.push(path),
                Ok(meta) if guarded && meta.file_type().is_symlink() => {
                    if at_id {
                        links.push(path);
                    }
                }
                Ok(meta) if guarded => {
                    if meta.is_dir() {
                        next.push(path);
                    }
                }
                Ok(_) => {
                    if path.is_dir() {
                        next.push(path);
                    }
                }
                Err(_) => {}
            };
            for dir in &level {
                if let [literal] = component.pieces.as_slice() {
                    take(dir.join(literal));
                    continue;
                }
                if !dir.is_dir() {
                    continue;
                }
                for entry in fs::read_dir(dir).map_err(|err| (dir.clone(), err))?.flatten() {
                    let name = entry.file_name();
                    if name.to_str().is_some_and(|name| pieces_match(&component.pieces, name)) {
                        take(entry.path());
                    }
                }
            }
            level = next;
        }
        level.extend(links);
        Ok(level)
    }

    /// Whether `path` is this ticket's: it matches, component for component, and no
    /// id of `kept`, the tickets the operation leaves as they stand, could stand
    /// where `{task_id}` stands in it. §FS-rhei-reset.2.1 §FS-rhei-remove.4.2
    fn owns<'k>(&self, path: &Path, kept: impl IntoIterator<Item = &'k str>) -> bool {
        self.matches(path) && !kept.into_iter().any(|other| self.could_stand(path, other))
    }

    /// Whether `path`, through the component holding the id or in full, reads as
    /// this pattern does.
    fn matches(&self, path: &Path) -> bool {
        self.names(path).is_some_and(|names| {
            names.iter().zip(&self.components).all(|(name, component)| pieces_match(&component.pieces, name))
        })
    }

    /// Whether `other`, another ticket's id, could stand where this ticket's id stands
    /// in `path`. §FS-rhei-reset.2.1
    fn could_stand(&self, path: &Path, other: &str) -> bool {
        self.names(path).is_some_and(|names| {
            names.iter().zip(&self.components).any(|(name, component)| {
                component.id_slot.as_ref().is_some_and(|(before, after)| {
                    id_could_stand(name, &self.id, before, after, other)
                })
            })
        })
    }

    /// `path`'s components under `root`: all it has, or, for a link it lists above
    /// its last component, those through the component holding the id.
    fn names<'p>(&self, path: &'p Path) -> Option<Vec<&'p str>> {
        let rest = path.strip_prefix(&self.root).ok()?;
        let names: Option<Vec<&str>> = rest.components().map(|part| part.as_os_str().to_str()).collect();
        let through_id = self.components.iter().position(|component| component.id_slot.is_some())? + 1;
        names.filter(|names| (through_id..=self.components.len()).contains(&names.len()))
    }
}

/// The one ownership rule, for a declared path and a prefix rhei writes alike:
/// whether `other`, an id longer than `id`, could stand where `id` stands in `name`,
/// one path component. It begins where the text before it reads as `before`, the
/// text declared before the id, and runs over a place `id` stands at, where the
/// text after `id` reads as `after`. Nothing after `other` is asked: a placeholder
/// reaches files of every naming, and one directory can hold several. A path such
/// an id of a kept ticket could stand in is ambiguous, and the kept ticket's claim
/// wins. §FS-rhei-reset.2.1 §FS-rhei-remove.4.2
fn id_could_stand(name: &str, id: &str, before: &[String], after: &[String], other: &str) -> bool {
    if other.len() <= id.len() || !other.contains(id) {
        return false;
    }
    let place = |at: usize| name.is_char_boundary(at) && pieces_match(before, &name[..at]);
    (0..=name.len()).filter(|&from| place(from) && name[from..].starts_with(other)).any(|from| {
        (from..=from + other.len() - id.len()).any(|at| {
            place(at) && name[at..].starts_with(id) && pieces_match(after, &name[at + id.len()..])
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
