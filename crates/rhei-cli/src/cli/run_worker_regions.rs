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
fn snapshot_region(text: &str, local_id: &str) -> Option<RegionSnapshot> {
    let mut in_code = false;
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
    })
}

/// Where the snapshot's region lies in `text` now, found by its two boundary
/// lines, each exactly once and the second after the first; refused otherwise.
// §FS-rhei-run.3.7.3
fn region_span(text: &str, snapshot: &RegionSnapshot) -> Result<RegionSpan, RestoreRefusal> {
    let lines: Vec<(usize, &str)> = region_lines_with_offsets(text).collect();
    let indices_of = |wanted: &str| -> Vec<usize> {
        lines
            .iter()
            .enumerate()
            .filter(|(_, (_, line))| *line == wanted)
            .map(|(at, _)| at)
            .collect()
    };
    let headings = indices_of(&snapshot.heading);
    let [heading] = headings[..] else {
        return Err(RestoreRefusal::Heading { found: headings.len() });
    };
    let (end, end_index) = match &snapshot.following {
        None => (text.len(), lines.len()),
        Some(following) => match indices_of(following)[..] {
            [at] if at > heading => (lines[at].0, at),
            ref found => return Err(RestoreRefusal::Following { found: found.len() }),
        },
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

/// The in-file id of the innermost task whose region holds 1-based `line`: the
/// last node heading before it, since a later heading of any depth would have
/// ended or nested that region first. §FS-rhei-run.3.7.6
fn region_owner_at(text: &str, line: usize) -> Option<String> {
    let mut in_code = false;
    let mut owner = None;
    for (_, current) in region_lines_with_offsets(text).take(line.saturating_sub(1)) {
        if let Some((_, id)) = node_heading_outside_code(current, &mut in_code) {
            owner = Some(id.to_string());
        }
    }
    owner
}

/// The task id, as the run prints it, of the task whose region holds `line`
/// of `file`: the in-file id under the id of the rhei that owns the file.
// §FS-rhei-run.3.7.6 §AR-rhei-panta.3
fn task_owning_line(file: &Path, line: usize) -> Option<String> {
    let local = region_owner_at(&fs::read_to_string(file).ok()?, line)?;
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
