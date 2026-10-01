mod viz;

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};

struct Example {
    name: &'static str,
    path: &'static str,
    state_machine: Option<&'static str>,
    runnable: bool,
}

const EXAMPLES: &[Example] = &[
    Example {
        name: "release-automation",
        path: "examples/release-automation.rhei.md",
        state_machine: None,
        runnable: false,
    },
    Example {
        name: "human-review-loop",
        path: "examples/human-review-loop.rhei.md",
        state_machine: None,
        runnable: false,
    },
    Example {
        name: "pm-onboarding-experiment",
        path: "examples/pm-onboarding-experiment.rhei.md",
        state_machine: None,
        runnable: false,
    },
    Example {
        name: "escaped-state-values",
        path: "examples/escaped-state-values.rhei.md",
        state_machine: Some("examples/states-with-spaces.yaml"),
        runnable: false,
    },
    Example {
        name: "claude-code",
        path: "examples/claude-code/plan.rhei.md",
        state_machine: Some("examples/claude-code/states.yaml"),
        runnable: false,
    },
    Example {
        name: "living-review-loop",
        path: "examples/living-review-loop",
        state_machine: Some("examples/living-review-loop/team-states.yaml"),
        runnable: false,
    },
    Example {
        name: "review-fix-visits",
        path: "examples/review-fix-visits",
        state_machine: Some("examples/review-fix-visits/states.yaml"),
        runnable: false,
    },
    Example {
        name: "agent-discussion",
        path: "examples/agent-discussion",
        state_machine: Some("examples/agent-discussion/discussion-states.yaml"),
        runnable: true,
    },
    Example {
        name: "analyze-and-dispatch",
        path: "examples/analyze-and-dispatch-example",
        state_machine: Some("examples/analyze-and-dispatch-example/states.yaml"),
        runnable: false,
    },
    Example {
        name: "parallel-worktrees",
        path: "examples/parallel-worktrees-example",
        state_machine: Some("examples/parallel-worktrees-example/states.yaml"),
        runnable: false,
    },
    Example {
        name: "multi-model-analysis",
        path: "examples/multi-model-analysis-example",
        state_machine: Some("examples/multi-model-analysis-example/states.yaml"),
        runnable: false,
    },
    Example {
        name: "spec-review",
        path: "examples/spec-review-example",
        state_machine: Some("examples/spec-review-example/states.yaml"),
        runnable: false,
    },
    Example {
        name: "subtree-supervision",
        path: "examples/subtree-supervision",
        state_machine: Some("examples/subtree-supervision/states.yaml"),
        runnable: true,
    },
    Example {
        name: "snapshot-continuation",
        path: "examples/snapshot-continuation",
        state_machine: Some("examples/snapshot-continuation/states.yaml"),
        runnable: false,
    },
    Example {
        name: "hourly-human-intervention",
        path: "examples/hourly-human-intervention-example",
        state_machine: Some("examples/hourly-human-intervention-example/states.yaml"),
        runnable: false,
    },
    Example {
        name: "ui-test-canonical",
        path: "examples/ui-test-canonical-example",
        state_machine: Some("examples/ui-test-canonical-example/states.yaml"),
        runnable: true,
    },
    // The shape pairs: the same work authored flat and nested, both runnable with the
    // mock agent, so the difference a run shows is the shape. §FS-rhei-shape.4
    Example {
        name: "shape-reproducer-flat",
        path: "examples/shape/reproducer/flat",
        state_machine: Some("examples/shape/reproducer/flat/states.yaml"),
        runnable: true,
    },
    Example {
        name: "shape-reproducer-nested",
        path: "examples/shape/reproducer/nested",
        state_machine: Some("examples/shape/reproducer/nested/states.yaml"),
        runnable: true,
    },
    Example {
        name: "shape-review-against-spec-flat",
        path: "examples/shape/review-against-spec/flat",
        state_machine: Some("examples/shape/review-against-spec/flat/states.yaml"),
        runnable: true,
    },
    Example {
        name: "shape-review-against-spec-nested",
        path: "examples/shape/review-against-spec/nested",
        state_machine: Some("examples/shape/review-against-spec/nested/states.yaml"),
        runnable: true,
    },
    Example {
        name: "shape-cve-category-flat",
        path: "examples/shape/cve-category/flat",
        state_machine: Some("examples/shape/cve-category/flat/states.yaml"),
        runnable: true,
    },
    Example {
        name: "shape-cve-category-nested",
        path: "examples/shape/cve-category/nested",
        state_machine: Some("examples/shape/cve-category/nested/states.yaml"),
        runnable: true,
    },
    Example {
        name: "shape-parts-of-a-feature-flat",
        path: "examples/shape/parts-of-a-feature/flat",
        state_machine: Some("examples/shape/parts-of-a-feature/flat/states.yaml"),
        runnable: true,
    },
    Example {
        name: "shape-parts-of-a-feature-nested",
        path: "examples/shape/parts-of-a-feature/nested",
        state_machine: Some("examples/shape/parts-of-a-feature/nested/states.yaml"),
        runnable: true,
    },
];

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask crate must live in workspace root")
        .to_path_buf()
}

fn usage() {
    eprintln!("Usage: cargo xtask <command> [args]");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  examples list                  List all known examples");
    eprintln!("  examples validate <name>       Validate one example");
    eprintln!("  examples validate --all        Validate every example");
    eprintln!("  examples run <name> [--viz]    Run a runnable example in a tmp copy");
    eprintln!("  examples viz <name> [--open]   Render HTML plan viz for one example");
    eprintln!("  examples viz --all             Render HTML viz for every example");
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let slice: Vec<&str> = args.iter().map(String::as_str).collect();
    match slice.as_slice() {
        ["examples", "list"] => {
            cmd_list();
            ExitCode::SUCCESS
        }
        ["examples", "validate", "--all"] => cmd_validate_all(),
        ["examples", "validate", name] => cmd_validate_one(name),
        ["examples", "run", name] => cmd_run(name, false),
        ["examples", "run", name, "--viz"] | ["examples", "run", "--viz", name] => {
            cmd_run(name, true)
        }
        ["examples", "viz", "--all"] => cmd_viz_all(),
        ["examples", "viz", name] => cmd_viz_one(name, false),
        ["examples", "viz", name, "--open"] | ["examples", "viz", "--open", name] => {
            cmd_viz_one(name, true)
        }
        _ => {
            usage();
            ExitCode::from(2)
        }
    }
}

fn cmd_list() {
    let width = EXAMPLES.iter().map(|e| e.name.len()).max().unwrap_or(0);
    for ex in EXAMPLES {
        let tag = if ex.runnable { "validate, run" } else { "validate" };
        println!("  {:<width$}  {}", ex.name, tag, width = width);
    }
}

fn find(name: &str) -> Option<&'static Example> {
    EXAMPLES.iter().find(|e| e.name == name)
}

fn cmd_validate_all() -> ExitCode {
    let mut failed: Vec<&str> = Vec::new();
    for ex in EXAMPLES {
        println!("==> validate {}", ex.name);
        if !run_validate(ex) {
            failed.push(ex.name);
        }
    }
    if failed.is_empty() {
        println!("\nAll {} examples validated.", EXAMPLES.len());
        ExitCode::SUCCESS
    } else {
        eprintln!("\nFailed: {}", failed.join(", "));
        ExitCode::FAILURE
    }
}

fn cmd_validate_one(name: &str) -> ExitCode {
    let Some(ex) = find(name) else {
        eprintln!("unknown example: {name}");
        return ExitCode::from(2);
    };
    if run_validate(ex) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn run_validate(ex: &Example) -> bool {
    let root = workspace_root();
    let mut cmd = Command::new(cargo());
    cmd.current_dir(&root).args(["run", "-q", "-p", "rhei-cli", "--bin", "rhei", "--"]);
    if let Some(sm) = ex.state_machine {
        cmd.args(["--state-machine", sm]);
    }
    cmd.args(["validate", ex.path]);
    cmd.status().map(|s| s.success()).unwrap_or(false)
}

fn cmd_run(name: &str, viz_after: bool) -> ExitCode {
    let Some(ex) = find(name) else {
        eprintln!("unknown example: {name}");
        return ExitCode::from(2);
    };
    if !ex.runnable {
        eprintln!("example '{}' is not runnable (validate only)", ex.name);
        return ExitCode::from(2);
    }
    let root = workspace_root();
    let src = root.join(ex.path);
    if !src.is_dir() {
        eprintln!("expected directory for runnable example: {}", src.display());
        return ExitCode::FAILURE;
    }
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let tmp = env::temp_dir().join(format!("rhei-xtask-{}-{stamp}", ex.name));
    let leaf = src.file_name().expect("example path has a file name");
    let dest = tmp.join(leaf);
    if let Err(err) = copy_dir_all(&src, &dest) {
        eprintln!("failed to copy example: {err}");
        return ExitCode::FAILURE;
    }
    let sm = ex.state_machine.expect("runnable examples must declare a state machine");
    let sm_rel = Path::new(sm)
        .strip_prefix(ex.path)
        .expect("runnable example's state machine must live under its directory");
    let sm_abs = dest.join(sm_rel);
    println!("==> running {} in {}", ex.name, dest.display());
    let status = Command::new(cargo())
        .current_dir(&root)
        .args(["run", "-q", "-p", "rhei-cli", "--bin", "rhei", "--"])
        .arg("--state-machine")
        .arg(&sm_abs)
        .arg("run")
        .arg(&dest)
        .status();
    let success = matches!(status, Ok(ref s) if s.success());
    if success {
        println!("\nArtifacts left in {}", dest.display());
    }
    if !success {
        if viz_after {
            eprintln!("skipping viz because the run failed");
        }
        return ExitCode::FAILURE;
    }

    if viz_after {
        match viz_run_output(ex.name, &dest, Some(&sm_abs)) {
            Ok(path) => {
                println!("Viz: {}", path.display());
                if let Err(err) = open_in_browser(&path) {
                    eprintln!("failed to open browser: {err}");
                    return ExitCode::FAILURE;
                }
            }
            Err(err) => {
                eprintln!("viz failed: {err}");
                return ExitCode::FAILURE;
            }
        }
    }

    ExitCode::SUCCESS
}

fn viz_run_output(
    example_name: &str,
    run_dir: &Path,
    machine_override: Option<&Path>,
) -> io::Result<PathBuf> {
    let out_dir = workspace_root().join("target/rhei-viz");
    fs::create_dir_all(&out_dir)?;
    let plans = viz::collect_plans(run_dir, example_name, machine_override)?;
    if plans.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no .rhei.md files under {}", run_dir.display()),
        ));
    }
    let html = viz::render_html(&plans);
    let out = out_dir.join(format!("{}-run.html", example_name));
    fs::write(&out, html)?;
    Ok(out)
}

fn cmd_viz_all() -> ExitCode {
    let out_dir = workspace_root().join("target/rhei-viz");
    if let Err(err) = fs::create_dir_all(&out_dir) {
        eprintln!("failed to create {}: {err}", out_dir.display());
        return ExitCode::FAILURE;
    }
    let mut failed: Vec<&str> = Vec::new();
    for ex in EXAMPLES {
        println!("==> viz {}", ex.name);
        match render_example(ex, &out_dir) {
            Ok(path) => println!("    {}", path.display()),
            Err(err) => {
                eprintln!("    failed: {err}");
                failed.push(ex.name);
            }
        }
    }
    println!("\nWrote {} HTML file(s) to {}", EXAMPLES.len() - failed.len(), out_dir.display());
    if failed.is_empty() {
        ExitCode::SUCCESS
    } else {
        eprintln!("Failed: {}", failed.join(", "));
        ExitCode::FAILURE
    }
}

fn cmd_viz_one(name: &str, open: bool) -> ExitCode {
    let Some(ex) = find(name) else {
        eprintln!("unknown example: {name}");
        return ExitCode::from(2);
    };
    let out_dir = workspace_root().join("target/rhei-viz");
    if let Err(err) = fs::create_dir_all(&out_dir) {
        eprintln!("failed to create {}: {err}", out_dir.display());
        return ExitCode::FAILURE;
    }
    match render_example(ex, &out_dir) {
        Ok(path) => {
            println!("{}", path.display());
            if open {
                if let Err(err) = open_in_browser(&path) {
                    eprintln!("failed to open browser: {err}");
                    return ExitCode::FAILURE;
                }
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("failed: {err}");
            ExitCode::FAILURE
        }
    }
}

fn render_example(ex: &Example, out_dir: &Path) -> io::Result<PathBuf> {
    let root = workspace_root();
    let src = root.join(ex.path);
    let machine_override = ex.state_machine.map(|path| root.join(path));
    let plans = viz::collect_plans(&src, ex.name, machine_override.as_deref())?;
    if plans.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no .rhei.md files under {}", src.display()),
        ));
    }
    let html = viz::render_html(&plans);
    let out = out_dir.join(format!("{}.html", ex.name));
    fs::write(&out, html)?;
    Ok(out)
}

fn open_in_browser(path: &Path) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    let tool = "open";
    #[cfg(target_os = "windows")]
    let tool = "cmd";
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let tool = "xdg-open";

    let mut cmd = Command::new(tool);
    #[cfg(target_os = "windows")]
    cmd.args(["/C", "start", ""]);
    cmd.arg(path);
    let status = cmd.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::new(io::ErrorKind::Other, format!("{tool} exited with status {status}")))
    }
}

fn cargo() -> PathBuf {
    env::var_os("CARGO").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("cargo"))
}

fn copy_dir_all(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &dst_path)?;
        } else if file_type.is_symlink() {
            copy_symlink(&entry.path(), &dst_path)?;
        } else {
            fs::copy(entry.path(), &dst_path)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn copy_symlink(src: &Path, dst: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(fs::read_link(src)?, dst)
}

#[cfg(windows)]
fn copy_symlink(src: &Path, dst: &Path) -> io::Result<()> {
    let target = fs::read_link(src)?;
    let target_path = if target.is_absolute() {
        target.clone()
    } else {
        src.parent().unwrap_or(Path::new(".")).join(&target)
    };
    if target_path.is_dir() {
        std::os::windows::fs::symlink_dir(target, dst)
    } else {
        std::os::windows::fs::symlink_file(target, dst)
    }
}
