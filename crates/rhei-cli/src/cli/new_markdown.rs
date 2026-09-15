// The markdown `rhei new` emits, and where it is inserted.
//
// Its own part because emitting a node and splicing it into an authored file
// are text concerns with no knowledge of ids, machines, or projects.

// §FS-rhei-new.2 §FS-rhei-new.3

/// Header fields shared by a single-file rhei and a workspace index.
struct RheiHeader<'a> {
    title: &'a str,
    states: Option<&'a str>,
    max_levels: Option<u8>,
    node_kinds: &'a [String],
    description: Option<&'a str>,
}

/// Render a rhei's header in the order the plan language fixes: heading,
/// `**States:**`, frontmatter, description. `with_tasks_section` appends the
/// `## Tasks` heading a single-file rhei requires and a workspace index must
/// not have.
// §FS-rhei-plan-language.1.1 §FS-rhei-plan-language.1.2
fn render_rhei_file(header: &RheiHeader<'_>, with_tasks_section: bool) -> String {
    let mut out = format!("# Rhei: {}\n", header.title.trim());
    if let Some(states) = header.states {
        out.push_str(&format!("**States:** {}\n", states.trim()));
    }
    if header.max_levels.is_some() || !header.node_kinds.is_empty() {
        out.push_str("\n---\nstructure:\n");
        if let Some(max_levels) = header.max_levels {
            out.push_str(&format!("  maxLevels: {max_levels}\n"));
        }
        if !header.node_kinds.is_empty() {
            let kinds: Vec<String> =
                header.node_kinds.iter().map(|kind| kind.trim().to_ascii_lowercase()).collect();
            out.push_str(&format!("  nodeKinds: [{}]\n", kinds.join(", ")));
        }
        out.push_str("---\n");
    }
    if let Some(description) = description_body(header.description) {
        out.push('\n');
        out.push_str(&description);
        out.push('\n');
    }
    if with_tasks_section {
        out.push_str("\n## Tasks\n");
    }
    out
}

/// Every metadata field a new ticket can carry, in plan-language order.
// §FS-rhei-plan-language.2: metadata = state, prior, provides, consumes, excludes,
// assignee, execution override.
struct TicketFields<'a> {
    kind: &'a str,
    local_id: &'a str,
    title: &'a str,
    state: &'a str,
    prior: &'a [String],
    provides: &'a [String],
    consumes: &'a [String],
    excludes: &'a [String],
    assignee: Option<&'a str>,
    model: Option<&'a str>,
    target: Option<&'a str>,
    description: Option<&'a str>,
}

/// Render one ticket node. The heading level follows the id depth, so a
/// rhei-local `1.2` is a `####` under its parent's `###`.
fn render_ticket(fields: &TicketFields<'_>) -> String {
    let depth = fields.local_id.split('.').count();
    let hashes = "#".repeat(depth + 2);
    let mut out = format!(
        "{hashes} {} {}: {}\n**State:** {}\n",
        title_case_kind(fields.kind),
        fields.local_id,
        fields.title.trim(),
        fields.state
    );
    for (label, values) in [
        ("Prior", fields.prior),
        ("Provides", fields.provides),
        ("Consumes", fields.consumes),
        ("Excludes", fields.excludes),
    ] {
        if !values.is_empty() {
            let joined =
                values.iter().map(|value| value.trim()).collect::<Vec<_>>().join(", ");
            out.push_str(&format!("**{label}:** {joined}\n"));
        }
    }
    for (label, value) in
        [("Assignee", fields.assignee), ("Model", fields.model), ("Target", fields.target)]
    {
        if let Some(value) = value {
            out.push_str(&format!("**{label}:** {}\n", value.trim()));
        }
    }
    if let Some(description) = description_body(fields.description) {
        out.push('\n');
        out.push_str(&description);
        out.push('\n');
    }
    out
}

/// Trim a description to a body worth writing, or `None` when it is blank.
fn description_body(description: Option<&str>) -> Option<String> {
    let body = description?.trim();
    (!body.is_empty()).then(|| body.to_string())
}

/// The line terminator a file already uses: CRLF when it is the majority, LF
/// otherwise.
///
/// A splice that re-joins with `\n` rewrites every line of a CRLF file, which
/// turns a three-line create into a whole-file diff and buries what was added.
// §FS-rhei-new.3.1
fn dominant_line_ending(raw: &str) -> &'static str {
    let crlf = raw.matches("\r\n").count();
    let lf = raw.matches('\n').count() - crlf;
    if crlf > lf { "\r\n" } else { "\n" }
}

/// Strip one line terminator from the end of `text`, CRLF before LF. `None`
/// when the text does not end on a line boundary.
fn strip_one_line_ending(text: &str) -> Option<&str> {
    text.strip_suffix("\r\n").or_else(|| text.strip_suffix('\n'))
}

/// True when `raw` already ends in a blank line, so a separator would make a
/// second one. Existing spacing is authored spacing: `rhei new` adds to it and
/// never rewrites it. §FS-rhei-new.3.1
fn ends_with_blank_line(raw: &str) -> bool {
    let Some(body) = strip_one_line_ending(raw) else {
        return false;
    };
    body.is_empty() || strip_one_line_ending(body).is_some()
}

/// Append `block` to `out` line by line, terminated with `eol`. The rendered
/// block always uses `\n`; the file it joins decides what is written.
// §FS-rhei-new.3.1
fn push_block_with(out: &mut String, block: &str, eol: &str) {
    for line in block.trim_end_matches('\n').split('\n') {
        out.push_str(line.trim_end_matches('\r'));
        out.push_str(eol);
    }
}

/// End `out` on a line boundary and then on one blank line, in the file's own
/// terminator — adding neither when the text already ends that way.
fn push_block_separator(out: &mut String, raw: &str, eol: &str) {
    if raw.is_empty() {
        return;
    }
    if !raw.ends_with('\n') {
        out.push_str(eol);
    }
    if !ends_with_blank_line(raw) {
        out.push_str(eol);
    }
}

/// Append a ticket at the end of a file, separated by one blank line. This is
/// where a top-level ticket goes: `## Tasks` is the final `##` chapter, so the
/// end of the file is the end of the section. §FS-rhei-new.3.1
fn append_ticket(raw: &str, block: &str) -> String {
    let eol = dominant_line_ending(raw);
    let mut out = String::with_capacity(raw.len() + block.len() + 8);
    out.push_str(raw);
    push_block_separator(&mut out, raw, eol);
    push_block_with(&mut out, block, eol);
    out
}

/// Every line of `raw` as `(byte offset, text without its terminator)`.
///
/// Offsets rather than a `Vec<&str>` of lines: splicing by offset copies the
/// bytes on either side through unchanged, which is what the spec promises and
/// what re-joining split lines cannot do.
// §FS-rhei-new.3.1
fn line_offsets(raw: &str) -> impl Iterator<Item = (usize, &str)> {
    raw.split_inclusive('\n').scan(0usize, |offset, line| {
        let start = *offset;
        *offset += line.len();
        Some((start, line.trim_end_matches('\n').trim_end_matches('\r')))
    })
}

/// Insert a ticket immediately after `parent_local_id`'s existing subtree, so
/// the file stays in id order. `None` when the parent's heading is not in this
/// file. §FS-rhei-new.3.1
fn insert_ticket_after_subtree(raw: &str, parent_local_id: &str, block: &str) -> Option<String> {
    let mut in_code_block = false;
    let mut parent_hashes: Option<usize> = None;
    let mut subtree_end: Option<usize> = None;

    for (start, line) in line_offsets(raw) {
        let Some((hashes, id)) = node_heading_outside_code(line, &mut in_code_block) else {
            continue;
        };
        match parent_hashes {
            None if id == parent_local_id => parent_hashes = Some(hashes),
            // The subtree ends at the first heading no deeper than the parent.
            Some(parent) if hashes <= parent => {
                subtree_end = Some(start);
                break;
            }
            _ => {}
        }
    }

    parent_hashes?;
    let (head, tail) = raw.split_at(subtree_end.unwrap_or(raw.len()));
    let eol = dominant_line_ending(raw);
    let mut out = String::with_capacity(raw.len() + block.len() + 8);
    out.push_str(head);
    push_block_separator(&mut out, head, eol);
    push_block_with(&mut out, block, eol);
    if !tail.is_empty() {
        out.push_str(eol);
        out.push_str(tail);
    }
    Some(out)
}
