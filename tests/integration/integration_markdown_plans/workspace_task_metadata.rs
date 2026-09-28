// The loader and the diagnostics behind a workspace task file's own metadata block
// (agent-grounds/rhei#318): where the merged map comes from, what the six refusals say, and the line
// a parse error in the task body still reports. §AR-rhei-panta.2 §FS-rhei-plan-language.1.4 §FS-rhei-validate.4.4

/// The workspace directory is named `wp` here too, so an assertion reads in the
/// ids the specification writes.
fn metadata_workspace(prefix: &str, index: &str, task_files: &[(&str, &str)]) -> (TestDir, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let ws = dir.join("wp");
    fs::create_dir_all(ws.join("tasks")).expect("create workspace dirs");
    fs::write(ws.join("index.rhei.md"), index).expect("write index");
    for (name, content) in task_files {
        fs::write(ws.join("tasks").join(name), content).expect("write task file");
    }
    fs::write(dir.join("states.yaml"), WORKSPACE_STATE_MACHINE).expect("write states");
    (dir, ws)
}

/// A task file's `metadata.tasks.<id>` entry, read out of the merged rhei the
/// loader produced — rhei-local on disk, so `first` rather than `wp.first`.
/// §AR-rhei-panta.2
fn loaded_task_metadata(ws: &Path, id: &str) -> Option<YamlValue> {
    let loaded = workspace::load_workspace(ws).expect("workspace should load");
    let metadata = loaded.rhei.metadata?;
    metadata
        .get(YamlValue::String("metadata".into()))?
        .as_mapping()?
        .get(YamlValue::String("tasks".into()))?
        .as_mapping()?
        .get(YamlValue::String(id.into()))
        .cloned()
}

fn validate_output(dir: &Path, target: &Path) -> String {
    let output = rhei_command()
        .arg("--state-machine")
        .arg(dir.join("states.yaml"))
        .arg("validate")
        .arg(target)
        .output()
        .expect("validate runs");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    // miette wraps its report and draws a gutter; flatten both so a needle
    // longer than a few words can still match.
    let flattened: String = combined.chars().filter(|ch| *ch != '\u{2502}').collect();
    flattened.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn validate_fails(dir: &Path, target: &Path) -> String {
    let output = rhei_command()
        .arg("--state-machine")
        .arg(dir.join("states.yaml"))
        .arg("validate")
        .arg(target)
        .output()
        .expect("validate runs");
    assert!(
        !output.status.success(),
        "validation should fail; got:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    validate_output(dir, target)
}

const ONE_TASK_BLOCK: &str = "---\nmetadata:\n  tasks:\n    first:\n      context: oracle-labs\n---\n\
                              \n### Task first: first item\n**State:** pending\n";

/// 8 · The loader reads the block as the rhei's metadata rather than dropping it
/// as text before the first heading. §AR-rhei-panta.2
#[test]
fn the_loader_merges_a_task_files_metadata_block_into_the_rhei() {
    let (dir, ws) = metadata_workspace(
        "wtm-load",
        "# Rhei: Workspace\n",
        &[("01-first.md", ONE_TASK_BLOCK)],
    );
    let _ = &dir;

    let entry = loaded_task_metadata(&ws, "first").expect("the task file's entry should be loaded");
    assert_eq!(
        entry.get(YamlValue::String("context".into())).and_then(YamlValue::as_str),
        Some("oracle-labs"),
        "the block's key belongs to the loaded rhei, not to the task body"
    );

    let loaded = workspace::load_workspace(&ws).expect("workspace should load");
    assert_eq!(loaded.rhei.tasks.len(), 1, "and it defines exactly the one task it declares");
    assert!(
        !loaded.rhei.tasks[0].content.contains("metadata"),
        "the block must not survive as task content; got:\n{}",
        loaded.rhei.tasks[0].content
    );
}

/// 13 · **Guard.** A task file with no block parses exactly as it does today:
/// no metadata, one task. §FS-rhei-plan-language.1.4
#[test]
fn a_task_file_with_no_block_still_loads_with_no_metadata() {
    let (dir, ws) = metadata_workspace(
        "wtm-no-block",
        "# Rhei: Workspace\n",
        &[("01-first.md", "### Task first: first item\n**State:** pending\n")],
    );
    let _ = &dir;

    let loaded = workspace::load_workspace(&ws).expect("workspace should load");
    assert!(loaded.rhei.metadata.is_none(), "an index with no frontmatter still yields none");
    assert_eq!(loaded.rhei.tasks.len(), 1);
}

/// 9 · **Guard.** A parse error in the *task body* of a file that carries a
/// block still reports that file's own line — with and without `structure`
/// declared in the index, because that is what changes the synthetic prefix the
/// task file is wrapped in. §FS-rhei-validate.4.4
#[test]
fn a_task_body_error_keeps_the_task_files_own_line_number_either_way() {
    let bad = "---\nmetadata:\n  tasks:\n    first:\n      context: oracle-labs\n---\n\n\
               ### Task first: first item\n**State** pending\n";
    for (prefix, index) in [
        ("wtm-line-plain", "# Rhei: Workspace\n"),
        (
            "wtm-line-structure",
            "# Rhei: Workspace\n\n---\nstructure:\n  maxLevels: 3\n  nodeKinds: [task]\n---\n",
        ),
    ] {
        let (dir, ws) = metadata_workspace(prefix, index, &[("01-first.md", bad)]);
        let reported = validate_fails(&dir, &ws);
        assert!(
            reported.contains("line 9") && reported.contains("01-first.md"),
            "the malformed `**State**` is line 9 of the task file; got:\n{reported}"
        );
    }
}

/// 4 · The same key in both places is refused, naming both files and the key.
/// The two spaces are disjoint by key, so the fix is to keep it in one of them.
/// §FS-rhei-validate.4.4
#[test]
fn an_overlapping_key_names_both_files_and_the_key() {
    let (dir, ws) = metadata_workspace(
        "wtm-overlap",
        "# Rhei: Workspace\n\n---\nmetadata:\n  tasks:\n    first:\n      context: personal\n---\n",
        &[("01-first.md", ONE_TASK_BLOCK)],
    );

    let reported = validate_fails(&dir, &ws);
    assert!(
        reported.contains("context")
            && reported.contains("index.rhei.md")
            && reported.contains("01-first.md"),
        "the overlap must name both files and the key; got:\n{reported}"
    );
}

/// 10 · `structure` in a task file is sent to `index.rhei.md` by name, because
/// it is the one plan-wide key an author plausibly writes into the wrong file;
/// any other top-level key gets the generic message. §FS-rhei-validate.4.4
#[test]
fn a_plan_wide_key_in_a_task_file_is_sent_to_the_index() {
    let (dir, ws) = metadata_workspace(
        "wtm-structure",
        "# Rhei: Workspace\n",
        &[(
            "01-first.md",
            "---\nstructure:\n  maxLevels: 5\n---\n\n### Task first: first item\n**State:** pending\n",
        )],
    );
    let reported = validate_fails(&dir, &ws);
    assert!(
        reported.contains("structure") && reported.contains("index.rhei.md"),
        "`structure` should be named and sent to the index; got:\n{reported}"
    );

    let (other_dir, other_ws) = metadata_workspace(
        "wtm-other-key",
        "# Rhei: Workspace\n",
        &[(
            "01-first.md",
            "---\nstates: custom\n---\n\n### Task first: first item\n**State:** pending\n",
        )],
    );
    let other = validate_fails(&other_dir, &other_ws);
    assert!(
        other.contains("states") && other.contains("metadata"),
        "any other top-level key should be named against the one key allowed; got:\n{other}"
    );
}

/// 11 · An entry for an id this file does not define is refused, and the message
/// says both places it could go instead. §FS-rhei-validate.4.4
#[test]
fn an_entry_for_a_foreign_id_is_refused() {
    let (dir, ws) = metadata_workspace(
        "wtm-foreign-id",
        "# Rhei: Workspace\n",
        &[
            (
                "01-first.md",
                "---\nmetadata:\n  tasks:\n    second:\n      context: personal\n---\n\n\
                 ### Task first: first item\n**State:** pending\n",
            ),
            ("02-second.md", "### Task second: second item\n**State:** pending\n"),
        ],
    );

    let reported = validate_fails(&dir, &ws);
    assert!(
        reported.contains("second") && reported.contains("index.rhei.md"),
        "the id and where the entry may go should both be named; got:\n{reported}"
    );
}

/// 12 · Malformed YAML inside the block is refused with the task file's own
/// line, rather than passed over as text. §FS-rhei-validate.4.4
#[test]
fn malformed_yaml_in_the_block_is_refused_with_the_task_files_line() {
    let (dir, ws) = metadata_workspace(
        "wtm-bad-yaml",
        "# Rhei: Workspace\n",
        &[(
            "01-first.md",
            "---\nmetadata:\n  tasks:\n      first:\n    context: [unclosed\n---\n\n\
             ### Task first: first item\n**State:** pending\n",
        )],
    );

    let reported = validate_fails(&dir, &ws);
    assert!(
        reported.contains("01-first.md"),
        "the refusal should name the task file; got:\n{reported}"
    );
}

/// Every `metadata.tasks` entry the loader produced, keyed as the merged map
/// keys it, so a test can assert that one task has exactly one entry however
/// the two files spell its id. §FS-rhei-plan-language.1.4
fn loaded_task_entries(ws: &Path) -> Vec<(String, YamlValue)> {
    let loaded = workspace::load_workspace(ws).expect("workspace should load");
    let Some(metadata) = loaded.rhei.metadata else {
        return Vec::new();
    };
    let Some(tasks) = metadata
        .get(YamlValue::String("metadata".into()))
        .and_then(YamlValue::as_mapping)
        .and_then(|section| section.get(YamlValue::String("tasks".into())))
        .and_then(YamlValue::as_mapping)
    else {
        return Vec::new();
    };
    tasks
        .iter()
        .map(|(key, value)| match key {
            YamlValue::String(id) => (id.clone(), value.clone()),
            other => (serde_yaml::to_string(other).unwrap_or_default().trim().to_string(), value.clone()),
        })
        .collect()
}

const NESTED_INDEX: &str = "# Rhei: Workspace\n\n---\nstructure:\n  maxLevels: 2\nmetadata:\n  tasks:\n    \"1.2\":\n      stateVisits:\n        pending: 4\n---\n";

const NESTED_TASKS: &str = "---\nmetadata:\n  tasks:\n    1.2:\n      context: oracle-labs\n---\n\n\
                            ### Task 1: parent\n**State:** pending\n\n\
                            #### Task 1.2: child\n**State:** pending\n";

/// 15 · A task id is one task however the two files spell it. The runtime writes
/// a multi-segment id into the index as a string while a bare `1.2:` authored in
/// a task file is a YAML float, so a merge that compared the raw keys filed one
/// task twice and let the later entry replace the index's — the runtime's own
/// `stateVisits` included. §FS-rhei-plan-language.1.4
#[test]
fn a_task_id_spelled_as_a_number_and_as_a_string_is_one_entry() {
    let (_dir, ws) =
        metadata_workspace("wtm-number-id", NESTED_INDEX, &[("01-first.md", NESTED_TASKS)]);

    let entries = loaded_task_entries(&ws);
    assert_eq!(
        entries.len(),
        1,
        "`1.2` and `\"1.2\"` name one task, so the merged map holds one entry; got:\n{entries:?}"
    );
    let (id, entry) = &entries[0];
    assert_eq!(id, "1.2");
    let entry = entry.as_mapping().expect("the merged entry is a mapping");
    assert!(
        entry.contains_key(YamlValue::String("context".into()))
            && entry.contains_key(YamlValue::String("stateVisits".into())),
        "both files' keys must survive the merge; got:\n{entry:?}"
    );
}

/// 15 · The same rule with the spellings swapped, and on a single-segment id,
/// which `task_id_yaml_key` keeps numeric in the index: a numeric index key and
/// a quoted task-file key are still one task. §FS-rhei-plan-language.1.4
#[test]
fn a_single_segment_numeric_id_merges_in_either_direction() {
    for (index_key, file_key) in [("\"3\"", "3"), ("3", "\"3\"")] {
        let index = format!(
            "# Rhei: Workspace\n\n---\nmetadata:\n  tasks:\n    {index_key}:\n      \
             stateVisits:\n        pending: 4\n---\n"
        );
        let task_file = format!(
            "---\nmetadata:\n  tasks:\n    {file_key}:\n      context: oracle-labs\n---\n\n\
             ### Task 3: an item\n**State:** pending\n"
        );
        let (_dir, ws) = metadata_workspace(
            "wtm-number-id-either",
            &index,
            &[("01-first.md", task_file.as_str())],
        );

        let entries = loaded_task_entries(&ws);
        assert_eq!(
            entries.len(),
            1,
            "index `{index_key}` and task file `{file_key}` name one task; got:\n{entries:?}"
        );
        let entry = entries[0].1.as_mapping().expect("the merged entry is a mapping");
        assert!(
            entry.contains_key(YamlValue::String("context".into()))
                && entry.contains_key(YamlValue::String("stateVisits".into())),
            "index `{index_key}` and task file `{file_key}` must merge; got:\n{entry:?}"
        );
    }
}

/// 16 · The overlap refusal reads the two spellings as one task too, so a key
/// set in both places is named rather than silently resolved by whichever entry
/// the map kept last. §FS-rhei-validate.4.4
#[test]
fn an_overlapping_key_is_refused_across_the_two_spellings() {
    let (dir, ws) = metadata_workspace(
        "wtm-number-id-overlap",
        NESTED_INDEX,
        &[("01-first.md", &NESTED_TASKS.replace("context: oracle-labs", "stateVisits:\n        pending: 9"))],
    );

    let reported = validate_fails(&dir, &ws);
    assert!(
        reported.contains("stateVisits")
            && reported.contains("index.rhei.md")
            && reported.contains("01-first.md")
            && reported.contains("`1.2`"),
        "the overlap must name both files, the key and the id as authored; got:\n{reported}"
    );
}

/// 17 · A key under `metadata` other than `tasks` — `taks` for `tasks` — is the
/// same mistake as a stray top-level key and is named the same way, because it
/// is otherwise the one block still accepted and then discarded.
/// §FS-rhei-validate.4.4
#[test]
fn a_key_under_metadata_other_than_tasks_is_refused() {
    let (dir, ws) = metadata_workspace(
        "wtm-metadata-typo",
        "# Rhei: Workspace\n",
        &[(
            "01-first.md",
            "---\nmetadata:\n  taks:\n    first:\n      context: oracle-labs\n---\n\n\
             ### Task first: first item\n**State:** pending\n",
        )],
    );

    let reported = validate_fails(&dir, &ws);
    assert!(
        reported.contains("`metadata.taks`") && reported.contains("`tasks`"),
        "the refusal must name the key at the depth it was written; got:\n{reported}"
    );
}

/// 18 · A key names the task its own text spells, not the one the scalar it
/// parses as rounds to: `1.10` is task `1.10`, whose index entry it merges
/// with, and not the `1.1` that a parsed float would deliver it to — a sibling
/// that exists whenever a parent has ten children.
/// §FS-rhei-plan-language.1.4
#[test]
fn an_id_whose_float_form_rounds_names_the_task_it_spells() {
    let mut body = String::from("### Task 1: parent\n**State:** pending\n\n");
    for n in 1..=10 {
        body.push_str(&format!("#### Task 1.{n}: child {n}\n**State:** pending\n\n"));
    }
    let (_dir, ws) = metadata_workspace(
        "wtm-rounding-id",
        "# Rhei: Workspace\n\n---\nstructure:\n  maxLevels: 2\nmetadata:\n  tasks:\n    \
         \"1.10\":\n      stateVisits:\n        pending: 4\n---\n",
        &[(
            "01-first.md",
            &format!("---\nmetadata:\n  tasks:\n    1.10:\n      context: oracle-labs\n---\n\n{body}"),
        )],
    );

    let entries = loaded_task_entries(&ws);
    assert_eq!(
        entries.len(),
        1,
        "the file's `1.10` and the index's `\"1.10\"` are one task; got:\n{entries:?}"
    );
    let (id, entry) = &entries[0];
    assert_eq!(id, "1.10", "the entry belongs to the task the key spells");
    let entry = entry.as_mapping().expect("the merged entry is a mapping");
    assert!(
        entry.contains_key(YamlValue::String("context".into()))
            && entry.contains_key(YamlValue::String("stateVisits".into())),
        "and it carries both files' keys; got:\n{entry:?}"
    );
    assert!(
        loaded_task_metadata(&ws, "1.1").is_none(),
        "the sibling the float form rounds to must get nothing"
    );
}

/// 19 · A key no id can be read out of names no task, so it is refused rather
/// than stepped past and discarded — and where the file does define a task by
/// that name, the entry is that task's. §FS-rhei-validate.4.4
#[test]
fn a_key_that_reads_as_no_id_is_refused_and_a_real_one_is_not() {
    let block = "---\nmetadata:\n  tasks:\n    true:\n      context: oracle-labs\n---\n\n";
    let (dir, ws) = metadata_workspace(
        "wtm-unreadable-id",
        "# Rhei: Workspace\n",
        &[("01-first.md", &format!("{block}### Task first: first item\n**State:** pending\n"))],
    );
    let reported = validate_fails(&dir, &ws);
    assert!(
        reported.contains("`metadata.tasks.true`") && reported.contains("names no task"),
        "the key must be named rather than passed over; got:\n{reported}"
    );

    let (_dir, ws) = metadata_workspace(
        "wtm-unreadable-id-real",
        "# Rhei: Workspace\n",
        &[("01-first.md", &format!("{block}### Task true: an item\n**State:** pending\n"))],
    );
    let entry = loaded_task_metadata(&ws, "true").expect("the task named `true` has the entry");
    assert_eq!(
        entry.get(YamlValue::String("context".into())).and_then(YamlValue::as_str),
        Some("oracle-labs"),
        "a task really named `true` keeps its own entry"
    );
}

/// 20 · One file spelling one id twice is one task written twice, so it is
/// refused where it is written — naming that file and the id, rather than
/// sending the reader to an `index.rhei.md` the entries never came from.
/// §FS-rhei-validate.4.4
#[test]
fn one_file_spelling_one_id_twice_is_refused_naming_that_file() {
    for second in ["context: b", "priority: high"] {
        let (dir, ws) = metadata_workspace(
            "wtm-one-id-twice",
            "# Rhei: Workspace\n",
            &[(
                "01-first.md",
                &format!(
                    "---\nmetadata:\n  tasks:\n    3:\n      context: a\n    \"3\":\n      \
                     {second}\n---\n\n### Task 3: three\n**State:** pending\n"
                ),
            )],
        );

        let reported = validate_fails(&dir, &ws);
        assert!(
            reported.contains("two entries for task `3`") && reported.contains("01-first.md"),
            "the refusal must name the file that spells the id twice; got:\n{reported}"
        );
        assert!(
            !reported.contains("index.rhei.md"),
            "and must not send the reader to a file the entries never came from; got:\n{reported}"
        );
    }
}
