    // What laying a project template into a project that already exists must
    // not write through: a symbolic link among the paths it writes, or among
    // their parents inside the project. Its own part because the answer is a
    // refusal, made the same way on every platform before anything is written.

    // §FS-rhei-library.2.2 §REQ-cross-platform.2

    /// Refuse a root write that would write through a symbolic link, or a
    /// member's settings hoist that would when `settings` names its path. A
    /// bundle linked to a template's own directory would otherwise have that
    /// template's source overwritten with its rendered text. A refusal rather
    /// than a replacement of the link, because removing a link to a directory
    /// is a different operation on each platform. §FS-rhei-library.2.2
    /// §REQ-cross-platform.2
    fn refuse_linked_root_writes(
        writes: &UnionWrites,
        settings: Option<&Path>,
        project: &Path,
    ) -> MietteResult<()> {
        let links = links_written_through(writes, settings, project);
        if links.is_empty() {
            return Ok(());
        }
        Err(linked_root_refusal(&links))
    }

    /// Every link the lay would write through — a path it writes, or a parent
    /// of one below the project root — each once, in path order.
    fn links_written_through(
        writes: &UnionWrites,
        settings: Option<&Path>,
        project: &Path,
    ) -> Vec<PathBuf> {
        let written = writes
            .files
            .iter()
            .map(|(path, _)| path.as_path())
            .chain(writes.copies.iter().map(|(_, dst)| dst.as_path()))
            .chain(settings);
        let mut links: Vec<PathBuf> = Vec::new();
        for path in written {
            for at in path.ancestors().take_while(|at| *at != project && at.starts_with(project)) {
                let linked =
                    fs::symlink_metadata(at).is_ok_and(|meta| meta.file_type().is_symlink());
                if linked && !links.iter().any(|link| link == at) {
                    links.push(at.to_path_buf());
                }
            }
        }
        links.sort();
        links
    }

    /// The refusal naming each link and where it points, with the one command
    /// that removes the link and nothing it points to. §FS-rhei-library.2.2
    fn linked_root_refusal(links: &[PathBuf]) -> Report {
        let pointing = |link: &Path| {
            fs::read_link(link).map_or_else(|_| "?".to_owned(), |target| display_slash(&target))
        };
        let reason = "a rebind copies the machine's bundle — it never writes through a link";
        let (subject, remedy) = match links {
            [link] => (
                format!(
                    "'{}' is a symbolic link to '{}', and {reason}",
                    link_slash(link),
                    pointing(link)
                ),
                format!(
                    "remove the link ({} removes the link, not what it points to)",
                    unlink_command(links)
                ),
            ),
            _ => {
                let mut subject = format!(
                    "{} paths this would write through are symbolic links, and {reason}:\n",
                    links.len()
                );
                let width = links.iter().map(|link| link_slash(link).len()).max().unwrap_or(0);
                for link in links {
                    let shown = link_slash(link);
                    subject.push_str(&format!("\n  {shown:<width$} -> {}", pointing(link)));
                }
                let remedy = format!(
                    "remove the links ({} removes the links, not what they point to)",
                    unlink_command(links)
                );
                (subject, remedy)
            }
        };
        miette!(help = format!("{remedy}, then run this again. Nothing was written."), "{subject}")
    }

    /// The command that removes `links` and nothing they point to, spelled for
    /// the platform: `rm` on Unix; on Windows `rmdir` for a link to a directory
    /// and `del` for one to a file, since `Remove-Item` on a directory link may
    /// recurse into what it points to.
    fn unlink_command(links: &[PathBuf]) -> String {
        if !cfg!(windows) {
            let paths: Vec<String> =
                links.iter().map(|link| shell_quote(&link_slash(link))).collect();
            return format!("`rm {}`", paths.join(" "));
        }
        links
            .iter()
            .map(|link| {
                let directory = fs::metadata(link).is_ok_and(|meta| meta.is_dir());
                let verb = if directory { "rmdir" } else { "del" };
                format!("`{verb} \"{}\"`", link_path(link).display())
            })
            .collect::<Vec<_>>()
            .join(" then ")
    }

    /// A link as a message names it: its parent spelled as every report spells
    /// a path, and its own name joined on. `display_path` resolves a path it
    /// cannot shorten as written, and resolving a link names what it points
    /// to, so a command built from that spelling would remove the target.
    /// §FS-rhei-library.2.2
    fn link_path(link: &Path) -> PathBuf {
        let (Some(parent), Some(name)) = (link.parent(), link.file_name()) else {
            return link.to_path_buf();
        };
        if parent.as_os_str().is_empty() {
            return link.to_path_buf();
        }
        let parent = display_path(parent);
        if parent == Path::new(".") { PathBuf::from(name) } else { parent.join(name) }
    }

    /// [`link_path`] spelled with `/`, as `display_slash` spells any other path.
    /// §REQ-cross-platform.2
    fn link_slash(link: &Path) -> String {
        let shown = link_path(link).to_string_lossy().into_owned();
        if cfg!(windows) { shown.replace('\\', "/") } else { shown }
    }
