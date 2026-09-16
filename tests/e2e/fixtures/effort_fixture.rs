// Portable argv recorder for state-effort scenarios. §FS-rhei-agents.2.2

use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

fn prompt_from(args: &[String], stdin: &str) -> String {
    args.iter()
        .find(|arg| arg.starts_with("# Task ") && arg.contains("\n## State: "))
        .cloned()
        .unwrap_or_else(|| stdin.to_string())
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

fn safe(value: &str) -> String {
    value.chars().map(|ch| if ch.is_ascii_alphanumeric() || ch == '-' { ch } else { '-' }).collect()
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut stdin = String::new();
    io::stdin().read_to_string(&mut stdin).expect("read prompt stdin");
    let prompt = prompt_from(&args, &stdin);

    let root = execution_root(&prompt).unwrap_or_else(|| {
        PathBuf::from(env::var("RHEI_CHECKOUT_ROOT").expect("RHEI_CHECKOUT_ROOT"))
    });
    let state = env::var("RHEI_STATE").expect("RHEI_STATE");
    let agent = env::var("RHEI_AGENT").expect("RHEI_AGENT");
    let identity = env::var("RHEI_MODEL")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| env::var("RHEI_TARGET_SLUG").ok().filter(|value| !value.is_empty()))
        .unwrap_or_else(|| agent.clone());
    let record_dir = root.join("runtime/effort-argv");
    fs::create_dir_all(&record_dir).expect("create argv record directory");
    let record = record_dir.join(format!("{}-{}.txt", safe(&state), safe(&identity)));

    let mut text = format!("agent={agent}\nstate={state}\n");
    for arg in &args {
        let visible = if arg.starts_with("# Task ") && arg.contains("\n## State: ") {
            "<PROMPT>"
        } else {
            arg
        };
        text.push_str("arg=");
        text.push_str(visible);
        text.push('\n');
    }
    fs::write(record, text).expect("write argv record");

    if let Some(path) = result_path(&prompt) {
        let path = if path.is_absolute() { path } else { root.join(path) };
        if let Some(parent) = Path::new(&path).parent() {
            fs::create_dir_all(parent).expect("create result directory");
        }
        fs::write(path, "Fixture agent completed.\n").expect("write task result");
    }
}
