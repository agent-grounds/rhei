//! A compiled stand-in for pi that is as strict about the command line as pi is.
//! §FS-rhei-agents.2.2.2 §FS-rhei-agents.2
//!
//! It refuses a bare `--` the way pi does: `Error: Unknown option: --` and
//! exit 1. Otherwise it reads its whole stdin as the prompt, records how many
//! bytes arrived and the size of its largest argument, and writes the result
//! the prompt asks for. It is its own binary rather than `effort-fixture`,
//! because that one is also run by profiles that end in `--` and accepts it.

use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

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
    // Answer only version queries before any task input. §FS-rhei-evidence.7.1
    if args == ["--version"] {
        println!("pi-stdin-fixture 0.0.0");
        return;
    }
    if args.iter().any(|arg| arg == "--") {
        eprintln!("Error: Unknown option: --");
        std::process::exit(1);
    }

    let mut prompt = String::new();
    io::stdin().read_to_string(&mut prompt).expect("read prompt stdin");

    let root = execution_root(&prompt).unwrap_or_else(|| {
        PathBuf::from(env::var("RHEI_CHECKOUT_ROOT").expect("RHEI_CHECKOUT_ROOT"))
    });
    let record = serde_json::json!({
        "stdin_bytes": prompt.len(),
        "largest_arg_bytes": args.iter().map(String::len).max().unwrap_or(0),
        "argv": args,
    });
    let record_dir = root.join("runtime/pi-stdin");
    fs::create_dir_all(&record_dir).expect("create record directory");
    fs::write(record_dir.join("delivery.json"), record.to_string()).expect("write delivery record");

    if let Some(path) = result_path(&prompt) {
        let path = if path.is_absolute() { path } else { root.join(path) };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create result directory");
        }
        fs::write(path, "The brief arrived on stdin.\n").expect("write task result");
    }
}
