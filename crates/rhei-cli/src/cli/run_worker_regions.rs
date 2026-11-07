// What a worker may edit, as text: a task's region in the file that declares
// it, the snapshot of that region the run keeps at spawn, where a loader's line
// falls, and the restore that puts one region back and nothing else.
//
// Its own part because these are facts about one file's text, with no
// knowledge of which worker is running or what the run does next; the registry
// beside it decides that.

// §AR-source-file-size.3 §FS-rhei-run.3.7

/// A task's region as it stood when its worker spawned. §FS-rhei-run.3.7.1
#[derive(Clone, Debug, PartialEq, Eq)]
struct RegionSnapshot {
    /// The task's own heading line, without its line ending.
    heading: String,
    /// The region's text: the heading line up to, not including, `following`.
    text: String,
    /// The heading line that followed the region, or `None` when it ended the file.
    following: Option<String>,
    /// The role the file is read in, so the restore finds the heading the snapshot
    /// did. §FS-rhei-plan-language.2.1
    role: rhei_core::header::DocumentRole,
}

/// Where one region sits in a file's current text: byte offsets, and 1-based
/// lines with `end_line` exclusive.
#[derive(Debug, PartialEq, Eq)]
struct RegionSpan {
    start: usize,
    end: usize,
    start_line: usize,
    end_line: usize,
}

impl RegionSpan {
    fn holds_line(&self, line: usize) -> bool {
        (self.start_line..self.end_line).contains(&line)
    }
}

/// Which boundary a restore could not find exactly once, and how often it did.
/// §FS-rhei-run.3.7.3
#[derive(Debug, PartialEq, Eq)]
enum RestoreRefusal {
    Heading { found: usize },
    Following { found: usize },
}

impl RestoreRefusal {
    fn describe(&self, snapshot: &RegionSnapshot) -> String {
        match self {
            RestoreRefusal::Heading { found } => format!(
                "its heading `{}` is in the file {found} time(s), not once",
                snapshot.heading
            ),
            RestoreRefusal::Following { found } => format!(
                "the heading that followed it, `{}`, is in the file {found} time(s), not once after it",
                snapshot.following.as_deref().unwrap_or_default()
            ),
        }
    }
}

/// Each line of `text` with the byte offset it starts at, line ending excluded.
fn region_lines_with_offsets(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut offset = 0;
    text.split_inclusive('\n').map(move |raw| {
        let start = offset;
        offset += raw.len();
        (start, raw.trim_end_matches(['\n', '\r']))
    })
}

/// The region of the task whose in-file id is `local_id`: from its node heading
/// up to the next node heading at the same or a shallower depth, or the end of
/// the file. `None` unless the heading is declared exactly once.
// §FS-rhei-run.3.7.1
fn snapshot_region(
    text: &str,
    role: rhei_core::header::DocumentRole,
    local_id: &str,
) -> Option<RegionSnapshot> {
    // §FS-rhei-plan-language.2.1: quoted headings do not delimit worker regions.
    let mut in_code = rhei_core::header::DocumentFence::of_role(role, text);
    let mut own: Option<(usize, usize, &str)> = None;
    let mut declared = 0;
    let mut following: Option<(usize, &str)> = None;
    for (offset, line) in region_lines_with_offsets(text) {
        let Some((depth, id)) = node_heading_outside_code(line, &mut in_code) else { continue };
        if let Some((_, own_depth, _)) = own {
            if following.is_none() && depth <= own_depth {
                following = Some((offset, line));
            }
        }
        if id == local_id {
            declared += 1;
            own.get_or_insert((offset, depth, line));
        }
    }
    let (start, _, heading) = own.filter(|_| declared == 1)?;
    let end = following.map_or(text.len(), |(offset, _)| offset);
    Some(RegionSnapshot {
        heading: heading.to_string(),
        text: text[start..end].to_string(),
        following: following.map(|(_, line)| line.to_string()),
        role,
    })
}

/// The region of `route`'s task in its file as it is on disk, read in the role
/// `loaded` read that file in. §FS-rhei-run.3.7.1 §FS-rhei-plan-language.2.1
fn file_region(loaded: &LoadedPlan, route: &TaskRoute) -> Option<RegionSnapshot> {
    let text = fs::read_to_string(&route.task_file).ok()?;
    snapshot_region(&text, loaded.document_role(&route.task_file), &route.local_id)
}

/// Where the snapshot's region lies in `text` now, found by its two boundary
/// lines, each exactly once and the second after the first; refused otherwise.
///
/// The task's heading is a node heading as [`snapshot_region`] reads one, so a
/// copy of its line in a value or a fence is not it. The following heading is
/// matched by text alone, with no fence state carried out of the worker's region,
/// which may hold the fence the worker left open; so it is counted over the whole
/// file, and its one line must be below the task's heading.
// §FS-rhei-run.3.7.3
fn region_span(text: &str, snapshot: &RegionSnapshot) -> Result<RegionSpan, RestoreRefusal> {
    let lines: Vec<(usize, &str)> = region_lines_with_offsets(text).collect();
    let mut in_code = rhei_core::header::DocumentFence::of_role(snapshot.role, text);
    let headings: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, (_, line))| {
            // Read every line, so the fence state is right on each one.
            let is_heading = node_heading_outside_code(line, &mut in_code).is_some();
            is_heading && *line == snapshot.heading
        })
        .map(|(at, _)| at)
        .collect();
    let [heading] = headings[..] else {
        return Err(RestoreRefusal::Heading { found: headings.len() });
    };
    let (end, end_index) = match &snapshot.following {
        None => (text.len(), lines.len()),
        Some(following) => {
            // Counted over the whole file: read without fences, a second copy anywhere
            // means the line below may be a later task's quote. §FS-rhei-run.3.7.3
            let found: Vec<usize> =
                (0..lines.len()).filter(|&at| lines[at].1 == following.as_str()).collect();
            match found[..] {
                [at] if at > heading => (lines[at].0, at),
                ref found => return Err(RestoreRefusal::Following { found: found.len() }),
            }
        }
    };
    Ok(RegionSpan {
        start: lines[heading].0,
        end,
        start_line: heading + 1,
        end_line: end_index + 1,
    })
}

/// `text` with the snapshot's region put back, and the text that replaced it.
/// Every byte outside the region is kept. §FS-rhei-run.3.7.3
fn restore_region(
    text: &str,
    snapshot: &RegionSnapshot,
) -> Result<(String, String), RestoreRefusal> {
    let span = region_span(text, snapshot)?;
    let restored = format!("{}{}{}", &text[..span.start], snapshot.text, &text[span.end..]);
    Ok((restored, text[span.start..span.end].to_string()))
}

/// The in-file id of the innermost task under whose heading 1-based `line`
/// falls: the last node heading before it, since a later heading of any depth
/// would have ended or nested that task first. A task's own heading line is not
/// under it, so line 1 of a task file has no owner, though `region_span` counts
/// that line in the task's region. §FS-rhei-run.3.7.6
fn region_owner_at(
    text: &str,
    role: rhei_core::header::DocumentRole,
    line: usize,
) -> Option<String> {
    // §FS-rhei-plan-language.2.1: a quoted heading cannot change the owning task.
    let mut in_code = rhei_core::header::DocumentFence::of_role(role, text);
    let mut owner = None;
    for (_, current) in region_lines_with_offsets(text).take(line.saturating_sub(1)) {
        if let Some((_, id)) = node_heading_outside_code(current, &mut in_code) {
            owner = Some(id.to_string());
        }
    }
    owner
}

/// The task id, as the run prints it, of the task under whose heading `line`
/// of `file` falls, `file` read in `role`: the in-file id under the id of the
/// rhei that owns the file. §FS-rhei-plan-language.2.1
// §FS-rhei-run.3.7.6 §AR-rhei-panta.3
fn task_owning_line(
    file: &Path,
    role: rhei_core::header::DocumentRole,
    line: usize,
) -> Option<String> {
    let local = region_owner_at(&fs::read_to_string(file).ok()?, role, line)?;
    let entry = file.ancestors().skip(1).find(|dir| workspace::is_workspace(dir)).unwrap_or(file);
    match workspace::rhei_id_for_path(entry) {
        Ok(rhei) => Some(format!("{rhei}.{local}")),
        Err(_) => Some(local),
    }
}

/// The reverted-text file beside an attempt's log: its name, `-attempt{n}`
/// included, with `.log` replaced by `.reverted.md`. §FS-rhei-run.3.7.7
fn reverted_text_path(log: &Path) -> PathBuf {
    log.with_extension("reverted.md")
}
