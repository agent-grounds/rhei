// `rhei viz` — render a self-contained Flow page for a plan or workspace,
// resolving the machine and building the same VizModel bundle as the live
// dashboard, inlined into the one asset. §FS-rhei-viz.7.2

fn viz_command(
    input: &Path,
    rhei_scope: &[String],
    state_machine: Option<&Path>,
    output: Option<&Path>,
    open: bool,
) -> MietteResult<()> {
    // Keep the rendered output in the same guarded operation as its source reads. §FS-rhei-recover.4
    let _root_guards = rhei_core::root_access::for_input(input).map_err(|err| miette!("{err}"))?;
    let key = input
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("plan")
        .to_string();

    let plans = rhei_viz::collect_plans(input, &key, state_machine)
        .map_err(|err| miette!(
            help = viz_path_help(),
            "failed to collect plans from {}: {err}", input.display()
        ))?;
    if plans.is_empty() {
        return Err(miette!(
            help = "point rhei viz at a .rhei.md plan or a directory that contains one.",
            "no .rhei.md plans found at {}", input.display()
        ));
    }

    // A member rhei renders its project's graph, filtered to its own tickets:
    // a cross-rhei prior is part of what governs this rhei, so the edge stays
    // and its far end is drawn as an out-of-scope neighbour. §FS-rhei-viz.7.3
    let plans = rhei_viz::narrow_bundle(plans, rhei_scope);

    let html = rhei_viz_model::render_static(&plans);
    let out = output.map(Path::to_path_buf).unwrap_or_else(|| default_viz_output(input));
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| file_io_report(parent, "failed to create", err))?;
    }
    std::fs::write(&out, html).map_err(|err| file_io_report(&out, "failed to write", err))?;
    println!("Wrote flow visualization to {}", out.display());

    if open {
        open_in_browser(&out)?;
    }
    Ok(())
}

/// Default output under the workspace's `runtime/` directory (the run-end freeze
/// location), so a generated view never lands next to a checked-in plan:
/// `runtime/rhei-viz.html` for a directory, `runtime/<stem>.html` for a file. §FS-rhei-viz.7.2
fn default_viz_output(input: &Path) -> PathBuf {
    if input.is_dir() {
        input.join("runtime").join("rhei-viz.html")
    } else {
        let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("rhei-viz");
        let dir = input.parent().unwrap_or(Path::new("."));
        dir.join("runtime").join(format!("{stem}.html"))
    }
}

fn open_in_browser(path: &Path) -> MietteResult<()> {
    let (program, lead): (&str, &[&str]) = if cfg!(target_os = "macos") {
        ("open", &[])
    } else if cfg!(target_os = "windows") {
        ("cmd", &["/C", "start", ""])
    } else {
        ("xdg-open", &[])
    };
    std::process::Command::new(program)
        .args(lead)
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|err| miette!(
            help = "the visualization was written to disk; open that file yourself, or re-run with --no-open.",
            "failed to open browser: {err}"
        ))
}
