// Portable stand-in for `claude` that keeps the `--mcp-config` file it is handed,
// so a test can read the file after the run. §FS-rhei-mcp-config-file
// A run of several tasks keeps one copy per task and state. §FS-rhei-task-tooling.2

use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

/// Where the copy goes. The test sets it on `rhei run`, whose agents inherit it.
const COPY_TO: &str = "RHEI_E2E_MCP_CONFIG_COPY";

/// Where the copies go when several invocations hand over a file: one per
/// invocation, named `<task-id>-<state>.json` like the run log beside it.
const COPY_INTO: &str = "RHEI_E2E_MCP_CONFIG_COPY_DIR";

/// The task id from the prompt's task heading, `# <Kind> <id>: <title>`. Rhei
/// strips `RHEI_TASK_ID` from every agent, and a state no transition finishes
/// the task from has no `## Result` in its prompt, so the heading is the one
/// place every prompt names the task. §FS-rhei-agents.4
fn task_id(prompt: &str) -> Option<&str> {
    let heading = prompt.lines().next()?.strip_prefix("# ")?;
    Some(heading.split_once(' ')?.1.split_once(':')?.0)
}

fn result_path(prompt: &str) -> Option<PathBuf> {
    let result = prompt.split_once("\n## Result\n")?.1;
    result
        .lines()
        .find_map(|line| line.trim().strip_prefix("- `")?.strip_suffix('`').map(PathBuf::from))
}

fn execution_root(prompt: &str) -> Option<PathBuf> {
    let prefix = "You are working in a rhei-managed plan at `";
    let path = prompt
        .lines()
        .find_map(|line| line.strip_prefix(prefix)?.strip_suffix("`.").map(PathBuf::from))?;
    if path.extension().is_some() {
        path.parent().map(Path::to_path_buf)
    } else {
        Some(path)
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut prompt = String::new();
    io::stdin().read_to_string(&mut prompt).expect("read prompt stdin");

    // No copy at all says rhei passed no file, which the test reports as such.
    let copy = match env::var_os(COPY_INTO) {
        Some(dir) => {
            let task = task_id(&prompt).expect("a task heading naming the task in the prompt");
            let state = env::var("RHEI_STATE").expect("RHEI_STATE");
            PathBuf::from(dir).join(format!("{task}-{state}.json"))
        }
        None => PathBuf::from(env::var_os(COPY_TO).expect(COPY_TO)),
    };
    if let Some(config) = args.iter().position(|arg| arg == "--mcp-config").map(|i| &args[i + 1]) {
        fs::copy(config, &copy).expect("copy the --mcp-config file");
    }

    if let Some(path) = result_path(&prompt) {
        let path = if path.is_absolute() {
            path
        } else {
            execution_root(&prompt).expect("execution root in the prompt").join(path)
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create result directory");
        }
        fs::write(path, "Fixture agent completed.\n").expect("write task result");
    }
}
