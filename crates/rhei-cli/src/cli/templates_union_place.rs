    // Placing a template's tickets: re-parenting their ids, deepening their
    // headings, rewriting the references they make to each other, and writing
    // them where `rhei new` writes a ticket.
    //
    // One re-parenting with two callers — `under:` inside a template and
    // `--into <rhei>.<task>` outside it — is the property that makes "a
    // template works at any level" a fact about one code path.

    // §FS-rhei-library.12 §FS-rhei-library.14.1 §AR-rhei-library.6.2

    /// One ticket-bearing file a template contributes, with its ids read out.
    #[derive(Debug, Clone)]
    struct PartTickets {
        /// `001-coordinate.md` becomes `coordinate`: the name its placed file
        /// keeps under a number of the host's. §FS-rhei-new.3.1
        slug: String,
        /// The markdown, with any leading frontmatter taken off.
        body: String,
        /// The file's own `metadata.tasks` block, keyed by ticket id.
        /// §FS-rhei-plan-language.1.4
        metadata: BTreeMap<String, YamlValue>,
        /// Every id the body declares, in document order.
        ids: Vec<String>,
    }

    /// Read every ticket-bearing file of a rendered template, in plan order.
    fn read_part_tickets(root: &Path) -> MietteResult<Vec<PartTickets>> {
        let tasks_dir = root.join("tasks");
        if !tasks_dir.is_dir() {
            let plan = root.join("plan.rhei.md");
            if !plan.is_file() {
                return Ok(Vec::new());
            }
            let raw = fs::read_to_string(&plan)
                .map_err(|err| file_io_report(&plan, "failed to read the template's plan", err))?;
            let body = raw.split_once("## Tasks").map_or(String::new(), |(_, rest)| {
                rest.trim_start_matches('\n').to_owned()
            });
            return Ok(vec![part_tickets("plan", &body)]);
        }
        let mut entries: Vec<PathBuf> = fs::read_dir(&tasks_dir)
            .map_err(|err| file_io_report(&tasks_dir, "failed to read the template's tasks", err))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
            .collect();
        entries.sort();
        let mut files = Vec::new();
        for path in entries {
            let raw = fs::read_to_string(&path)
                .map_err(|err| file_io_report(&path, "failed to read a template task file", err))?;
            let stem = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or("task");
            files.push(part_tickets(file_slug(stem), &raw));
        }
        Ok(files)
    }

    /// `001-coordinate` becomes `coordinate`: the number is the host's to
    /// assign, the name is the template author's. §FS-rhei-new.3.1
    fn file_slug(stem: &str) -> &str {
        stem.split_once('-')
            .filter(|(digits, rest)| !rest.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
            .map_or(stem, |(_, rest)| rest)
    }

    /// Split a rendered task file into its own metadata block and its body.
    fn part_tickets(slug: &str, raw: &str) -> PartTickets {
        let (front, body) = split_frontmatter(raw);
        let metadata = front
            .and_then(|text| serde_yaml::from_str::<YamlValue>(&text).ok())
            .and_then(|value| task_metadata_entries(&value))
            .unwrap_or_default();
        let ids = declared_ids(&body);
        PartTickets { slug: slug.to_owned(), body, metadata, ids }
    }

    /// A document's leading `---` frontmatter, and everything after it.
    fn split_frontmatter(raw: &str) -> (Option<String>, String) {
        let body = raw.strip_prefix("---\n").or_else(|| raw.strip_prefix("---\r\n"));
        let Some(body) = body else {
            return (None, raw.to_owned());
        };
        match body.split_once("\n---") {
            Some((front, rest)) => (
                Some(front.to_owned()),
                rest.trim_start_matches('\n').trim_start_matches("\r\n").to_owned(),
            ),
            None => (None, raw.to_owned()),
        }
    }

    /// A plan's frontmatter block, which sits after the header rather than at
    /// the top of the file the way a task file's own block does.
    /// §FS-rhei-plan-language.1.1
    fn plan_frontmatter(raw: &str) -> Option<String> {
        let lines: Vec<&str> = raw.lines().collect();
        let open = lines.iter().position(|line| line.trim_end() == "---")?;
        if lines[..open].iter().any(|line| line.starts_with("## ")) {
            return None;
        }
        let close = lines[open + 1..].iter().position(|line| line.trim_end() == "---")?;
        Some(lines[open + 1..open + 1 + close].join("\n"))
    }

    /// The `metadata.tasks` entries of a parsed frontmatter block.
    fn task_metadata_entries(value: &YamlValue) -> Option<BTreeMap<String, YamlValue>> {
        let tasks = value.get("metadata")?.get("tasks")?.as_mapping()?;
        Some(
            tasks
                .iter()
                .filter_map(|(key, value)| Some((key.as_str()?.to_owned(), value.clone())))
                .collect(),
        )
    }

    /// Every node heading id a markdown body declares, in document order.
    fn declared_ids(body: &str) -> Vec<String> {
        let mut in_code_block = false;
        body.lines()
            .filter_map(|line| {
                node_heading_outside_code(line, &mut in_code_block).map(|(_, id)| id.to_owned())
            })
            .collect()
    }

    /// Re-parent one template's tickets under `parent`, or leave them at the
    /// top level when it is `None`.
    ///
    /// This is the single re-parenting: `under:` calls it against the including
    /// template's tree, `--into <rhei>.<task>` calls it against the host's, and
    /// the two compose by being applied in turn. A second code path here would
    /// be the defect the design exists to prevent.
    /// §FS-rhei-library.12 §FS-rhei-library.14.1 §AR-rhei-library.6.2
    fn reparent(files: &mut [PartTickets], parent: Option<&str>) {
        let Some(parent) = parent else {
            return;
        };
        let known: BTreeSet<String> =
            files.iter().flat_map(|file| file.ids.iter().cloned()).collect();
        let placed = |id: &str| format!("{parent}.{id}");
        for file in files.iter_mut() {
            file.body = reparent_body(&file.body, parent, &known);
            file.ids = file.ids.iter().map(|id| placed(id)).collect();
            file.metadata = file
                .metadata
                .iter()
                .map(|(id, value)| {
                    let key =
                        if known.contains(id) { placed(id) } else { id.clone() };
                    (key, value.clone())
                })
                .collect();
        }
    }

    /// The markdown half of a re-parenting: headings gain the prefix and a
    /// level of depth, and every reference to a template task is rewritten to
    /// the placed id. §FS-rhei-library.12
    fn reparent_body(body: &str, parent: &str, known: &BTreeSet<String>) -> String {
        let mut in_code_block = false;
        let mut out = String::with_capacity(body.len() + 64);
        for line in body.split_inclusive('\n') {
            let text = line.trim_end_matches('\n').trim_end_matches('\r');
            let eol = &line[text.len()..];
            if let Some((hashes, id)) = node_heading_outside_code(text, &mut in_code_block) {
                if known.contains(id) {
                    let placed = format!("{parent}.{id}");
                    let depth = placed.split('.').count();
                    let rest = &text[hashes..];
                    let replaced = replace_first_id(rest, id, &placed);
                    out.push_str(&"#".repeat(depth + 2));
                    out.push_str(&replaced);
                    out.push_str(eol);
                    continue;
                }
            }
            out.push_str(&reparent_reference_line(text, parent, known, in_code_block));
            out.push_str(eol);
        }
        out
    }

    /// Replace the id token of a heading body, which is the word before the
    /// first colon. Only the id moves: the kind and the title are the author's.
    fn replace_first_id(rest: &str, id: &str, placed: &str) -> String {
        match rest.split_once(':') {
            Some((prefix, tail)) => {
                let head = prefix.strip_suffix(id).unwrap_or(prefix);
                format!("{head}{placed}:{tail}")
            }
            None => rest.to_owned(),
        }
    }

    /// `**Prior:**` and `**Consumes:**` naming a template task are rewritten to
    /// the placed id; anything else — a cross-rhei id, an id no template task
    /// declares — is left exactly as written. §FS-rhei-library.12
    fn reparent_reference_line(
        text: &str,
        parent: &str,
        known: &BTreeSet<String>,
        in_code_block: bool,
    ) -> String {
        if in_code_block {
            return text.to_owned();
        }
        for label in ["**Prior:**", "**Consumes:**"] {
            let Some(values) = text.trim_start().strip_prefix(label) else {
                continue;
            };
            let indent = &text[..text.len() - text.trim_start().len()];
            let rewritten: Vec<String> = values
                .split(',')
                .map(|value| {
                    let trimmed = value.trim();
                    let (id, suffix) = match trimmed.split_once(':') {
                        Some((id, export)) => (id, format!(":{export}")),
                        None => (trimmed, String::new()),
                    };
                    if known.contains(id) {
                        format!("{parent}.{id}{suffix}")
                    } else {
                        trimmed.to_owned()
                    }
                })
                .collect();
            return format!("{indent}{label} {}", rewritten.join(", "));
        }
        text.to_owned()
    }
