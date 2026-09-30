    // Writing a union: where the placed tickets go, what travels beside them,
    // the validation that happens before any of it lands, and the diff
    // `--dry-run` prints instead. §FS-rhei-library.10 §AR-rhei-library.6.4

    /// Place a template's tickets in a single-file rhei: under the parent's
    /// subtree, or at the end of `## Tasks`, which is the end of the file.
    /// §FS-rhei-new.3.1
    fn place_tickets_in_file(
        raw: &str,
        parent: Option<&str>,
        tickets: &[PartTickets],
    ) -> String {
        let mut out = raw.to_owned();
        for file in tickets {
            out = match parent {
                Some(parent) => insert_ticket_after_subtree(&out, parent, &file.body)
                    .unwrap_or_else(|| append_ticket(&out, &file.body)),
                None => append_ticket(&out, &file.body),
            };
        }
        out
    }

    /// The same for a Directory Workspace: a new three-digit file per top-level
    /// ticket, and an append to the parent's file for a subtask, because a task
    /// file owns its subtree. §FS-rhei-new.3.1
    fn place_workspace_tickets(
        host: &UnionHost,
        host_files: &[(PathBuf, Vec<String>)],
        tickets: &[PartTickets],
        writes: &mut UnionWrites,
    ) -> MietteResult<()> {
        let Some(parent) = host.parent.as_deref() else {
            let mut number = next_task_number(host_files);
            for file in tickets {
                let path = host.root.join("tasks").join(format!("{number:03}-{}.md", file.slug));
                writes.files.push((path, file.body.clone()));
                number += 1;
            }
            return Ok(());
        };
        let owner = host_files
            .iter()
            .find(|(_, ids)| ids.iter().any(|id| id == parent))
            .map(|(path, _)| path.clone())
            .ok_or_else(|| {
                miette!(
                    help = "run `rhei validate` on the target first, so the plan on disk and \
                            the ids agree.",
                    "no file in the target holds the heading for ticket '{parent}'"
                )
            })?;
        let mut text = read_text(&owner)?;
        for file in tickets {
            text = insert_ticket_after_subtree(&text, parent, &file.body).ok_or_else(|| {
                miette!(
                    help = internal_error_help(),
                    "internal error: the parent heading for '{parent}' moved mid-placement"
                )
            })?;
        }
        writes.files.push((owner, text));
        Ok(())
    }

    /// `prompt_templates/*.md`, `scripts/*` and the rest of a template's bundle
    /// travel beside the rhei's own, under rule 1: a file both sides ship must
    /// be the same file. `README.md` describes the template and stays behind.
    /// §FS-rhei-library.10 §FS-rhei-library.11.1
    fn copy_bundled_files(
        part_root: &Path,
        host_root: &Path,
        writes: &mut UnionWrites,
    ) -> MietteResult<()> {
        let skip = ["index.rhei.md", "plan.rhei.md", "states.yaml", "README.md", "tasks"];
        let mut entries: Vec<PathBuf> = fs::read_dir(part_root)
            .map_err(|err| file_io_report(part_root, "failed to read the rendered template", err))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect();
        entries.sort();
        for entry in entries {
            let Some(name) = entry.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if skip.contains(&name) {
                continue;
            }
            copy_bundled_path(&entry, &host_root.join(name), writes)?;
        }
        Ok(())
    }

    /// One bundled file or directory, refused when the host already has a file
    /// of that name with different bytes. §FS-rhei-library.11.1
    fn copy_bundled_path(
        src: &Path,
        dst: &Path,
        writes: &mut UnionWrites,
    ) -> MietteResult<()> {
        if src.is_dir() {
            let mut entries: Vec<PathBuf> = fs::read_dir(src)
                .map_err(|err| file_io_report(src, "failed to read a template directory", err))?
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .collect();
            entries.sort();
            for entry in entries {
                let Some(name) = entry.file_name() else { continue };
                copy_bundled_path(&entry, &dst.join(name), writes)?;
            }
            return Ok(());
        }
        if dst.exists() {
            let (left, right) = (fs::read(src), fs::read(dst));
            if let (Ok(left), Ok(right)) = (left, right) {
                if left == right {
                    return Ok(());
                }
            }
            return Err(miette!(
                help = "a union renames nothing: give the template's copy a name of its own.",
                "'{}' is shipped by the template and already exists in the target with \
                 different contents",
                display_path(dst).display()
            ));
        }
        writes.copies.push((src.to_path_buf(), dst.to_path_buf()));
        Ok(())
    }

    /// Validate the whole result before anything is written, by mirroring the
    /// target's plan files with the union applied. §AR-rhei-library.6.4
    fn validate_union(host: &UnionHost, writes: &UnionWrites) -> MietteResult<()> {
        let scratch = tempfile::tempdir().map_err(|err| {
            miette!(
                help = "a union is validated in a temp directory before it is written. Check \
                        that $TMPDIR exists and is writable.",
                "failed to create the validation directory: {err}"
            )
        })?;
        let mirror = scratch.path().join("target");
        mirror_plan_files(&host.root, &mirror)?;
        for (path, contents) in &writes.files {
            write_mirrored(&host.root, &mirror, path, contents.as_bytes())?;
        }
        for (src, dst) in &writes.copies {
            let bytes = fs::read(src)
                .map_err(|err| file_io_report(src, "failed to read a template file", err))?;
            write_mirrored(&host.root, &mirror, dst, &bytes)?;
        }
        let entrypoint = if host.single_file {
            let name = host.index.file_name().unwrap_or_default();
            mirror.join(name)
        } else {
            mirror
        };
        validation_warnings_or_error(&entrypoint, None).map(|_| ())
    }

    /// Copy the files a rhei's validation reads, and only those: a `runtime/`
    /// directory has nothing to say about whether a union is valid.
    fn mirror_plan_files(root: &Path, mirror: &Path) -> MietteResult<()> {
        fs::create_dir_all(mirror)
            .map_err(|err| file_io_report(mirror, "failed to create the validation copy", err))?;
        let mut entries: Vec<PathBuf> = fs::read_dir(root)
            .map_err(|err| file_io_report(root, "failed to read the target", err))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect();
        entries.sort();
        for entry in entries {
            let Some(name) = entry.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let relevant = name.ends_with(".rhei.md")
                || name == "states.yaml"
                || name == "tasks"
                || name == "prompt_templates"
                || name == "scripts";
            if !relevant {
                continue;
            }
            copy_tree(&entry, &mirror.join(name))?;
        }
        Ok(())
    }

    fn copy_tree(src: &Path, dst: &Path) -> MietteResult<()> {
        if src.is_file() {
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent)
                    .map_err(|err| file_io_report(parent, "failed to create a directory", err))?;
            }
            fs::copy(src, dst).map_err(|err| file_io_report(src, "failed to copy", err))?;
            return Ok(());
        }
        if !src.is_dir() {
            return Ok(());
        }
        fs::create_dir_all(dst)
            .map_err(|err| file_io_report(dst, "failed to create a directory", err))?;
        let mut entries: Vec<PathBuf> = fs::read_dir(src)
            .map_err(|err| file_io_report(src, "failed to read a directory", err))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect();
        entries.sort();
        for entry in entries {
            let Some(name) = entry.file_name() else { continue };
            copy_tree(&entry, &dst.join(name))?;
        }
        Ok(())
    }

    fn write_mirrored(
        root: &Path,
        mirror: &Path,
        path: &Path,
        bytes: &[u8],
    ) -> MietteResult<()> {
        let relative = path.strip_prefix(root).unwrap_or(path);
        let dest = mirror.join(relative);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| file_io_report(parent, "failed to create a directory", err))?;
        }
        fs::write(&dest, bytes)
            .map_err(|err| file_io_report(&dest, "failed to write the validation copy", err))
    }

    /// Write the union, which is the first moment anything about the target
    /// changes. §AR-rhei-library.6.4
    fn apply_union(writes: &UnionWrites) -> MietteResult<()> {
        for (path, contents) in &writes.files {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|err| file_io_report(parent, "failed to create a directory", err))?;
            }
            fs::write(path, contents)
                .map_err(|err| file_io_report(path, "failed to write", err))?;
        }
        for (src, dst) in &writes.copies {
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent)
                    .map_err(|err| file_io_report(parent, "failed to create a directory", err))?;
            }
            fs::copy(src, dst).map_err(|err| file_io_report(src, "failed to copy", err))?;
        }
        Ok(())
    }

    /// The diff `--dry-run` prints: one block per file, the added lines and
    /// nothing else, which is what the union's byte discipline is for.
    /// §FS-rhei-library.10 §FS-rhei-library.15.1
    fn print_union_diff(host: &UnionHost, part: &RenderedPart, writes: &UnionWrites) {
        println!(
            "Dry run: template '{}' would be placed into '{}'.",
            part.name,
            display_path(&host.index).display()
        );
        for (path, contents) in &writes.files {
            let before = fs::read_to_string(path).unwrap_or_default();
            let added = added_lines(&before, contents);
            if added.is_empty() {
                continue;
            }
            println!("--- {}", display_path(path).display());
            for line in added {
                println!("+{line}");
            }
        }
        for (_, dst) in &writes.copies {
            println!("--- {} (new)", display_path(dst).display());
        }
        for note in &writes.notes {
            println!("  {note}");
        }
    }

    /// The lines `after` has that `before` did not. A union only ever inserts,
    /// so walking the two in step is the whole of the comparison.
    fn added_lines(before: &str, after: &str) -> Vec<String> {
        let mut old = before.lines().peekable();
        let mut added = Vec::new();
        for line in after.lines() {
            if old.peek() == Some(&line) {
                old.next();
                continue;
            }
            added.push(line.to_owned());
        }
        added
    }
