    // Reading a `states.yaml` as *text* with its blocks located, and deciding
    // whether two authored definitions are the same thing.
    //
    // Its own part because a union keeps the host's bytes and inserts the
    // template's rendered lines at the end of each block, so every decision
    // below is about spans of the source rather than about a re-serialization.

    // §FS-rhei-library.2 §FS-rhei-library.7.1

    use std::ops::Range;

    /// One entry inside a top-level block: a named mapping key, or a sequence
    /// item keyed by the text a message would name it with.
    #[derive(Debug, Clone)]
    struct YamlEntry {
        key: String,
        range: Range<usize>,
        /// True when the entry's value opens a flow collection on the key's own
        /// line, as `by_type: { task: host }` does. §FS-rhei-library.7.4
        flow: bool,
    }

    /// One top-level block of a `states.yaml`, located in the source text.
    #[derive(Debug, Clone)]
    struct YamlBlock {
        /// Byte offset just past the block's last non-blank line, which is
        /// where an insertion goes. §FS-rhei-library.2
        body_end: usize,
        /// The indent its entries are written at, so an insertion matches.
        indent: usize,
        /// True when the block is a flow collection opened on its key's own
        /// line, which has no end to insert a line at. §FS-rhei-library.7.4
        flow: bool,
        entries: Vec<YamlEntry>,
    }

    /// Every top-level block of `text`, keyed by its YAML key.
    ///
    /// Block style with a consistent indent is what every authored machine in
    /// this repository uses, and what the writer emits. A block or entry whose
    /// value opens `{` or `[` on its key's own line is marked `flow`: it has no
    /// end to insert a line at, so the union refuses to extend it rather than
    /// splice block lines under it. §FS-rhei-library.7.4
    fn yaml_blocks(text: &str) -> IndexMap<String, YamlBlock> {
        let mut blocks: IndexMap<String, YamlBlock> = IndexMap::new();
        let mut current: Option<(String, usize, bool)> = None; // key, body end, flow
        let mut pending: Vec<(usize, usize)> = Vec::new(); // (offset, len) of body lines
        let mut offset = 0usize;
        let mut lines: Vec<(usize, &str)> = Vec::new();
        for line in text.split_inclusive('\n') {
            lines.push((offset, line));
            offset += line.len();
        }
        let flush = |blocks: &mut IndexMap<String, YamlBlock>,
                     current: &Option<(String, usize, bool)>,
                     pending: &[(usize, usize)]| {
            let Some((key, body_end, flow)) = current else {
                return;
            };
            blocks.insert(
                key.clone(),
                YamlBlock {
                    body_end: *body_end,
                    indent: pending.first().map_or(2, |(_, indent)| *indent),
                    flow: *flow,
                    entries: Vec::new(),
                },
            );
        };
        for (start, raw) in &lines {
            let line = raw.trim_end_matches('\n').trim_end_matches('\r');
            let indent = line.len() - line.trim_start().len();
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if indent == 0 {
                flush(&mut blocks, &current, &pending);
                pending.clear();
                current = trimmed
                    .split_once(':')
                    .filter(|(key, _)| is_yaml_key(key))
                    .map(|(key, value)| (key.to_owned(), start + raw.len(), opens_flow(value)));
                continue;
            }
            if let Some((_, body_end, _)) = current.as_mut() {
                *body_end = start + raw.len();
                pending.push((*start, indent));
            }
        }
        flush(&mut blocks, &current, &pending);
        for (key, block) in &mut blocks {
            block.entries = block_entries(&lines, text.len(), key, block.indent);
        }
        blocks
    }

    /// True when a key's value, as written on the key's own line, opens a flow
    /// collection. §FS-rhei-library.7.4
    fn opens_flow(value: &str) -> bool {
        let value = value.trim_start();
        value.starts_with('{') || value.starts_with('[')
    }

    /// True when `key` is a bare YAML mapping key rather than prose that
    /// happens to carry a colon.
    fn is_yaml_key(key: &str) -> bool {
        !key.is_empty()
            && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    }

    /// The entries of the block introduced by `key`, at `indent`.
    fn block_entries(
        lines: &[(usize, &str)],
        text_len: usize,
        key: &str,
        indent: usize,
    ) -> Vec<YamlEntry> {
        let mut entries: Vec<YamlEntry> = Vec::new();
        let mut inside = false;
        let mut sequence_index = 0usize;
        for (start, raw) in lines {
            let line = raw.trim_end_matches('\n').trim_end_matches('\r');
            let line_indent = line.len() - line.trim_start().len();
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if line_indent == 0 {
                if inside {
                    break;
                }
                inside = trimmed.split_once(':').is_some_and(|(k, _)| k == key);
                continue;
            }
            if !inside || line_indent != indent {
                continue;
            }
            if let Some(last) = entries.last_mut() {
                last.range.end = *start;
            }
            let (entry_key, flow) = if let Some(item) = trimmed.strip_prefix("- ") {
                let key = sequence_item_key(item, sequence_index);
                sequence_index += 1;
                (key, false)
            } else {
                match trimmed.split_once(':') {
                    Some((key, value)) => (
                        key.trim().trim_matches('"').trim_matches('\'').to_owned(),
                        opens_flow(value),
                    ),
                    None => continue,
                }
            };
            entries.push(YamlEntry { key: entry_key, range: *start..text_len, flow });
        }
        // The last entry runs to the end of its block, not of the file.
        if let Some(last) = entries.last_mut() {
            let end = lines
                .iter()
                .find(|(start, raw)| {
                    let line = raw.trim_end_matches('\n').trim_end_matches('\r');
                    *start > last.range.start
                        && !line.trim().is_empty()
                        && line.len() == line.trim_start().len()
                })
                .map_or(text_len, |(start, _)| *start);
            last.range.end = end;
        }
        entries
    }

    /// A sequence item's key: the `from`/`to` pair a transition is named by,
    /// falling back to its position so every item has one.
    fn sequence_item_key(item: &str, index: usize) -> String {
        match item.split_once(':') {
            Some((key, value)) if is_yaml_key(key.trim()) => {
                format!("{}={}", key.trim(), value.trim())
            }
            _ => format!("[{index}]"),
        }
    }

    /// `text`'s bytes for `range`, with every line's indent shifted from
    /// `from_indent` to `to_indent` and trailing blank lines dropped.
    fn reindent(text: &str, range: &Range<usize>, from: usize, to: usize) -> String {
        let mut out = String::new();
        for line in text[range.clone()].split_inclusive('\n') {
            let body = line.trim_end_matches('\n').trim_end_matches('\r');
            if body.trim().is_empty() {
                continue;
            }
            let stripped = body.strip_prefix(&" ".repeat(from)).unwrap_or(body);
            out.push_str(&" ".repeat(to));
            out.push_str(stripped);
            out.push('\n');
        }
        out
    }

    /// A YAML value reduced to a form two authors' spellings compare equal in:
    /// mapping order is dropped and `description` is not an operative field.
    /// §FS-rhei-library.3.1
    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Canon {
        Scalar(String),
        Seq(Vec<Canon>),
        Map(BTreeMap<String, Canon>),
    }

    fn canonical(value: &YamlValue) -> Canon {
        match value {
            YamlValue::Sequence(items) => Canon::Seq(items.iter().map(canonical).collect()),
            YamlValue::Mapping(map) => Canon::Map(
                map.iter()
                    .filter(|(key, _)| key.as_str() != Some("description"))
                    .map(|(key, value)| {
                        let key = key.as_str().map_or_else(
                            || format!("{key:?}"),
                            std::string::ToString::to_string,
                        );
                        (key, canonical(value))
                    })
                    .collect(),
            ),
            YamlValue::Null => Canon::Scalar(String::new()),
            other => Canon::Scalar(
                other.as_str().map_or_else(|| format!("{other:?}"), std::string::ToString::to_string),
            ),
        }
    }

    /// The first operative field two definitions disagree on, for the message
    /// rule 1 owes: a refusal that names the field is one an author can act on.
    /// §FS-rhei-library.3.1
    fn differing_field(left: &YamlValue, right: &YamlValue) -> Option<String> {
        let (Canon::Map(left), Canon::Map(right)) = (canonical(left), canonical(right)) else {
            return None;
        };
        left.keys()
            .chain(right.keys())
            .find(|key| left.get(*key) != right.get(*key))
            .cloned()
    }
